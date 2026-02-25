use kiwi_parser::{Action, Key, LayerMode, Modifiers};
use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::time::{Duration, Instant};
use tracing::{debug, trace};

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct HotkeyStep {
    pub key: Key,
    pub modifiers: Modifiers,
}

impl std::fmt::Display for HotkeyStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}{}{}",
            self.modifiers,
            if self.modifiers != Modifiers::empty() {
                "+"
            } else {
                ""
            },
            self.key
        )
    }
}

impl std::fmt::Debug for HotkeyStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}

impl HotkeyStep {
    pub fn new(key: Key, modifiers: Modifiers) -> Self {
        Self { key, modifiers }
    }
}

#[derive(Clone, Debug)]
pub struct LayerBehavior {
    pub name: Option<String>,
    pub mode: LayerMode,
    pub timeout_ms: Option<u32>,
    pub deactivate: Option<HotkeyStep>,
}

#[derive(Clone, Debug)]
pub struct HotkeyNode {
    pub action: Option<Action>,
    pub context: Option<String>,
    pub layer_behavior: Option<LayerBehavior>,
    pub children: HashMap<HotkeyStep, HotkeyNode>,
}

impl HotkeyNode {
    pub fn new() -> Self {
        Self {
            action: None,
            context: None,
            layer_behavior: None,
            children: HashMap::new(),
        }
    }
}

impl Default for HotkeyNode {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
struct ActiveLayer {
    name: Option<String>,
    path: Vec<HotkeyStep>,
    behavior: LayerBehavior,
    deadline: Option<Instant>,
}

#[derive(Clone)]
struct LayerRegistration {
    context: Option<String>,
    path: Vec<HotkeyStep>,
    behavior: LayerBehavior,
}

struct LookupHit {
    full_path: Vec<HotkeyStep>,
    action: Option<Action>,
    layer_behavior: Option<LayerBehavior>,
}

#[derive(Clone)]
pub struct HotkeyManager {
    root: HotkeyNode,
    active_layers: Vec<ActiveLayer>,
    active_activations: HashSet<HotkeyStep>,
    pending_deactivate_release: Option<HotkeyStep>,
    layer_registry: HashMap<String, LayerRegistration>,
}

pub struct ProcessResult {
    pub handled: bool,
    pub action: Option<Action>,
}

impl ProcessResult {
    fn keep() -> Self {
        Self {
            handled: false,
            action: None,
        }
    }

    fn consume(action: Option<Action>) -> Self {
        Self {
            handled: true,
            action,
        }
    }
}

impl HotkeyManager {
    pub fn new() -> Self {
        Self {
            root: HotkeyNode::new(),
            active_layers: Vec::new(),
            active_activations: HashSet::new(),
            pending_deactivate_release: None,
            layer_registry: HashMap::new(),
        }
    }

    pub fn bind(&mut self, sequence: Vec<HotkeyStep>, context: Option<String>, action: Action) {
        let mut node = &mut self.root;
        for step in sequence {
            node = node.children.entry(step).or_default();
        }
        node.action = Some(action);
        node.context = context;
    }

    pub fn register_layer(
        &mut self,
        sequence: Vec<HotkeyStep>,
        context: Option<String>,
        behavior: LayerBehavior,
    ) {
        let mut node = &mut self.root;
        for step in &sequence {
            node = node.children.entry(step.clone()).or_default();
        }
        node.context = context.clone();
        node.layer_behavior = Some(behavior.clone());

        if let Some(name) = &behavior.name {
            self.layer_registry.insert(
                name.clone(),
                LayerRegistration {
                    context,
                    path: sequence,
                    behavior,
                },
            );
        }
    }

    fn node_for_path(&self, path: &[HotkeyStep]) -> Option<&HotkeyNode> {
        let mut node = &self.root;
        for step in path {
            node = node.children.get(step)?;
        }
        Some(node)
    }

    fn lookup_in_scope(
        &self,
        path: &[HotkeyStep],
        step: &HotkeyStep,
        current_app: &str,
    ) -> Option<LookupHit> {
        let scope = self.node_for_path(path)?;
        let node = scope.children.get(step)?;

        if let Some(ctx) = &node.context
            && ctx != current_app
        {
            return None;
        }

        let mut full_path = path.to_vec();
        full_path.push(step.clone());

        Some(LookupHit {
            full_path,
            action: node.action.clone(),
            layer_behavior: node.layer_behavior.clone(),
        })
    }

    fn deadline_from_timeout(timeout_ms: Option<u32>) -> Option<Instant> {
        match timeout_ms {
            Some(ms) if ms > 0 => Some(Instant::now() + Duration::from_millis(ms as u64)),
            _ => None,
        }
    }

    fn pop_expired_layers(&mut self) {
        let now = Instant::now();
        while let Some(top) = self.active_layers.last() {
            let Some(deadline) = top.deadline else {
                break;
            };
            if now < deadline {
                break;
            }
            self.active_layers.pop();
        }
    }

    fn reset_top_deadline(&mut self) {
        if let Some(top) = self.active_layers.last_mut() {
            top.deadline = Self::deadline_from_timeout(top.behavior.timeout_ms);
        }
    }

    pub fn process(
        &mut self,
        key: Key,
        modifiers: Modifiers,
        is_down: bool,
        current_app: &str,
    ) -> ProcessResult {
        let step = HotkeyStep { key, modifiers };
        trace!("[{current_app}] {} {step}", if is_down { "↓" } else { "↑" });

        if !is_down {
            if self.pending_deactivate_release.as_ref() == Some(&step) {
                self.pending_deactivate_release = None;
                return ProcessResult::consume(None);
            }

            if self.active_activations.remove(&step) {
                return ProcessResult::consume(None);
            }
        }

        self.pop_expired_layers();

        if let Some(top) = self.active_layers.last()
            && top.behavior.deactivate.as_ref() == Some(&step)
        {
            self.active_layers.pop();
            if is_down {
                self.pending_deactivate_release = Some(step);
            }
            return ProcessResult::consume(None);
        }

        let depth = self.active_layers.len();
        let scope_path = self
            .active_layers
            .last()
            .map(|layer| layer.path.clone())
            .unwrap_or_default();

        let Some(hit) = self.lookup_in_scope(&scope_path, &step, current_app) else {
            if depth > 0 && is_down {
                // Miss while a layer is active always pops only one frame.
                // We only do this on keydown to avoid double-popping if keyup also misses.
                self.active_layers.pop();
            }
            return ProcessResult::keep();
        };

        if is_down {
            self.active_activations.insert(step.clone());
        }

        if let Some(layer_behavior) = hit.layer_behavior {
            debug!("Entering layer: {:?}", hit.full_path);

            // Entering a child layer counts as handled activity in the parent layer.
            if depth > 0 {
                self.reset_top_deadline();
            }

            let child = ActiveLayer {
                name: layer_behavior.name.clone(),
                path: hit.full_path,
                deadline: Self::deadline_from_timeout(layer_behavior.timeout_ms),
                behavior: layer_behavior,
            };
            self.active_layers.push(child);
            return ProcessResult::consume(None);
        }

        if let Some(action) = hit.action {
            debug!("Executing hotkey sequence: {:?}", hit.full_path);

            if depth > 0 {
                let mode = self.active_layers[depth - 1].behavior.mode;
                match mode {
                    LayerMode::Oneshot => {
                        self.active_layers.truncate(depth - 1);
                    }
                    LayerMode::Sticky => {
                        self.reset_top_deadline();
                    }
                }
            }

            return ProcessResult::consume(Some(action));
        }

        ProcessResult::consume(None)
    }

    pub fn registered_layer_names(&self) -> Vec<String> {
        let mut names: Vec<_> = self.layer_registry.keys().cloned().collect();
        names.sort();
        names
    }

    pub fn active_layer_names(&self) -> Vec<String> {
        self.active_layers
            .iter()
            .filter_map(|frame| frame.name.clone())
            .collect()
    }

    pub fn activate_layer(&mut self, name: &str, current_app: &str) -> Result<(), String> {
        let registration = self
            .layer_registry
            .get(name)
            .ok_or_else(|| format!("unknown layer: {name}"))?
            .clone();

        if let Some(ctx) = &registration.context
            && ctx != current_app
        {
            return Err(format!("layer '{name}' is scoped to app '{ctx}'"));
        }

        if !self.active_layers.is_empty() {
            self.reset_top_deadline();
        }

        self.active_layers.push(ActiveLayer {
            name: registration.behavior.name.clone(),
            path: registration.path,
            deadline: Self::deadline_from_timeout(registration.behavior.timeout_ms),
            behavior: registration.behavior,
        });
        Ok(())
    }

    pub fn clear_active_layers(&mut self) {
        self.active_layers.clear();
    }
}

impl Default for HotkeyManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{HotkeyManager, HotkeyStep, LayerBehavior};
    use kiwi_parser::{Action, Key, LayerMode, Modifiers};
    use std::thread;
    use std::time::Duration;

    fn step(ch: char) -> HotkeyStep {
        HotkeyStep::new(Key::Char(ch), Modifiers::NONE)
    }

    fn sticky(timeout: Option<u32>) -> LayerBehavior {
        LayerBehavior {
            name: None,
            mode: LayerMode::Sticky,
            timeout_ms: timeout,
            deactivate: None,
        }
    }

    fn oneshot(timeout: Option<u32>) -> LayerBehavior {
        LayerBehavior {
            name: None,
            mode: LayerMode::Oneshot,
            timeout_ms: timeout,
            deactivate: None,
        }
    }

    #[test]
    fn oneshot_exits_after_hit() {
        let mut mgr = HotkeyManager::new();
        mgr.register_layer(vec![step('a')], None, oneshot(None));
        mgr.bind(vec![step('a'), step('b')], None, Action::Reload);

        assert!(
            mgr.process(Key::Char('a'), Modifiers::NONE, true, "")
                .handled
        );
        let hit = mgr.process(Key::Char('b'), Modifiers::NONE, true, "");
        assert!(hit.action.is_some());

        let second = mgr.process(Key::Char('b'), Modifiers::NONE, true, "");
        assert!(!second.handled);
    }

    #[test]
    fn oneshot_exits_after_miss() {
        let mut mgr = HotkeyManager::new();
        mgr.register_layer(vec![step('a')], None, oneshot(None));
        mgr.bind(vec![step('a'), step('b')], None, Action::Reload);

        assert!(
            mgr.process(Key::Char('a'), Modifiers::NONE, true, "")
                .handled
        );
        let miss = mgr.process(Key::Char('z'), Modifiers::NONE, true, "");
        assert!(!miss.handled);

        let after = mgr.process(Key::Char('b'), Modifiers::NONE, true, "");
        assert!(!after.handled);
    }

    #[test]
    fn sticky_stays_after_hit() {
        let mut mgr = HotkeyManager::new();
        mgr.register_layer(vec![step('a')], None, sticky(None));
        mgr.bind(vec![step('a'), step('b')], None, Action::Reload);

        assert!(
            mgr.process(Key::Char('a'), Modifiers::NONE, true, "")
                .handled
        );
        assert!(
            mgr.process(Key::Char('b'), Modifiers::NONE, true, "")
                .action
                .is_some()
        );
        assert!(
            mgr.process(Key::Char('b'), Modifiers::NONE, true, "")
                .action
                .is_some()
        );
    }

    #[test]
    fn nested_miss_returns_to_parent() {
        let mut mgr = HotkeyManager::new();
        mgr.register_layer(vec![step('a')], None, sticky(None));
        mgr.register_layer(vec![step('a'), step('c')], None, sticky(None));
        mgr.bind(vec![step('a'), step('x')], None, Action::Reload);
        mgr.bind(vec![step('a'), step('c'), step('y')], None, Action::Quit);

        assert!(
            mgr.process(Key::Char('a'), Modifiers::NONE, true, "")
                .handled
        );
        assert!(
            mgr.process(Key::Char('c'), Modifiers::NONE, true, "")
                .handled
        );

        let miss = mgr.process(Key::Char('z'), Modifiers::NONE, true, "");
        assert!(!miss.handled);

        let parent_hit = mgr.process(Key::Char('x'), Modifiers::NONE, true, "");
        assert!(matches!(parent_hit.action, Some(Action::Reload)));
    }

    #[test]
    fn deactivate_is_consumed() {
        let mut mgr = HotkeyManager::new();
        mgr.register_layer(
            vec![step('a')],
            None,
            LayerBehavior {
                name: None,
                mode: LayerMode::Sticky,
                timeout_ms: None,
                deactivate: Some(step('d')),
            },
        );
        mgr.bind(vec![step('a'), step('x')], None, Action::Reload);

        assert!(
            mgr.process(Key::Char('a'), Modifiers::NONE, true, "")
                .handled
        );
        assert!(
            mgr.process(Key::Char('d'), Modifiers::NONE, true, "")
                .handled
        );
        assert!(
            mgr.process(Key::Char('d'), Modifiers::NONE, false, "")
                .handled
        );

        let after = mgr.process(Key::Char('x'), Modifiers::NONE, true, "");
        assert!(!after.handled);
    }

    #[test]
    fn timeout_expires_layer() {
        let mut mgr = HotkeyManager::new();
        mgr.register_layer(vec![step('a')], None, sticky(Some(5)));
        mgr.bind(vec![step('a'), step('x')], None, Action::Reload);

        assert!(
            mgr.process(Key::Char('a'), Modifiers::NONE, true, "")
                .handled
        );
        thread::sleep(Duration::from_millis(10));

        let after_timeout = mgr.process(Key::Char('x'), Modifiers::NONE, true, "");
        assert!(!after_timeout.handled);
    }

    #[test]
    fn timeout_resets_on_handled_key() {
        let mut mgr = HotkeyManager::new();
        mgr.register_layer(vec![step('a')], None, sticky(Some(20)));
        mgr.bind(vec![step('a'), step('x')], None, Action::Reload);

        assert!(
            mgr.process(Key::Char('a'), Modifiers::NONE, true, "")
                .handled
        );
        thread::sleep(Duration::from_millis(15));

        assert!(
            mgr.process(Key::Char('x'), Modifiers::NONE, true, "")
                .action
                .is_some()
        );
        thread::sleep(Duration::from_millis(15));

        // Still active because handled key reset timeout.
        assert!(
            mgr.process(Key::Char('x'), Modifiers::NONE, true, "")
                .action
                .is_some()
        );
    }

    #[test]
    fn timeout_does_not_reset_on_unhandled_event() {
        let mut mgr = HotkeyManager::new();
        mgr.register_layer(vec![step('a')], None, sticky(Some(20)));
        mgr.bind(vec![step('a'), step('x')], None, Action::Reload);

        assert!(
            mgr.process(Key::Char('a'), Modifiers::NONE, true, "")
                .handled
        );
        thread::sleep(Duration::from_millis(15));

        // Unhandled key-up should not reset layer timeout.
        let _ = mgr.process(Key::Char('q'), Modifiers::NONE, false, "");
        thread::sleep(Duration::from_millis(10));

        let after_timeout = mgr.process(Key::Char('x'), Modifiers::NONE, true, "");
        assert!(!after_timeout.handled);
    }
    #[test]
    fn up_only_event_executes_action() {
        let mut mgr = HotkeyManager::new();
        mgr.bind(vec![step('a')], None, Action::Reload);

        // Simulate a key-up without a preceding key-down for 'a'.
        let hit = mgr.process(Key::Char('a'), Modifiers::NONE, false, "");
        assert!(matches!(hit.action, Some(Action::Reload)));
    }
}
