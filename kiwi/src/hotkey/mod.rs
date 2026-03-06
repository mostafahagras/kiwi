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
    pub global_action: Option<Action>,
    pub app_actions: HashMap<String, Action>,
    pub global_layer_behavior: Option<LayerBehavior>,
    pub app_layer_behaviors: HashMap<String, LayerBehavior>,
    pub children: HashMap<HotkeyStep, HotkeyNode>,
}

impl HotkeyNode {
    pub fn new() -> Self {
        Self {
            global_action: None,
            app_actions: HashMap::new(),
            global_layer_behavior: None,
            app_layer_behaviors: HashMap::new(),
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
    context: Option<String>,
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
    context: Option<String>,
}

#[derive(Clone)]
pub struct HotkeyManager {
    root: HotkeyNode,
    active_layers: Vec<ActiveLayer>,
    active_activations: HashSet<HotkeyStep>,
    observed_downs: HashSet<HotkeyStep>,
    pending_deactivate_release: Option<HotkeyStep>,
    layer_registry: HashMap<String, LayerRegistration>,
    last_app: Option<String>,
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
            observed_downs: HashSet::new(),
            pending_deactivate_release: None,
            layer_registry: HashMap::new(),
            last_app: None,
        }
    }

    pub fn bind(&mut self, sequence: Vec<HotkeyStep>, context: Option<String>, action: Action) {
        let mut node = &mut self.root;
        for step in sequence {
            node = node.children.entry(step).or_default();
        }
        match context {
            Some(app) => {
                node.app_actions.insert(app, action);
            }
            None => {
                node.global_action = Some(action);
            }
        }
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
        match context.clone() {
            Some(app) => {
                node.app_layer_behaviors.insert(app, behavior.clone());
            }
            None => {
                node.global_layer_behavior = Some(behavior.clone());
            }
        }

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

        let mut full_path = path.to_vec();
        full_path.push(step.clone());

        if let Some(layer_behavior) = node.app_layer_behaviors.get(current_app).cloned() {
            return Some(LookupHit {
                full_path,
                action: None,
                layer_behavior: Some(layer_behavior),
                context: Some(current_app.to_string()),
            });
        }

        if let Some(action) = node.app_actions.get(current_app).cloned() {
            return Some(LookupHit {
                full_path,
                action: Some(action),
                layer_behavior: None,
                context: None,
            });
        }

        if let Some(layer_behavior) = node.global_layer_behavior.clone() {
            return Some(LookupHit {
                full_path,
                action: None,
                layer_behavior: Some(layer_behavior),
                context: None,
            });
        }

        if let Some(action) = node.global_action.clone() {
            return Some(LookupHit {
                full_path,
                action: Some(action),
                layer_behavior: None,
                context: None,
            });
        }

        None
    }

    fn sync_app_context(&mut self, current_app: &str) {
        if self.last_app.as_deref() == Some(current_app) {
            return;
        }

        if let Some(idx) = self
            .active_layers
            .iter()
            .position(|frame| matches!(&frame.context, Some(ctx) if ctx != current_app))
        {
            self.active_layers.truncate(idx);
            self.pending_deactivate_release = None;
        }

        self.last_app = Some(current_app.to_string());
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
        self.sync_app_context(current_app);
        let had_observed_down = if is_down {
            self.observed_downs.insert(step.clone());
            false
        } else {
            self.observed_downs.remove(&step)
        };

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

        let (depth, hit) = loop {
            let depth = self.active_layers.len();
            let scope_path = self
                .active_layers
                .last()
                .map(|layer| layer.path.clone())
                .unwrap_or_default();

            if let Some(hit) = self.lookup_in_scope(&scope_path, &step, current_app) {
                break (depth, hit);
            }

            if is_down && depth > 0 {
                // Same-event fallback: pop one frame and retry from parent/root.
                self.active_layers.pop();
                continue;
            }

            return ProcessResult::keep();
        };

        if is_down {
            self.active_activations.insert(step.clone());
        }

        if let Some(layer_behavior) = hit.layer_behavior {
            // We allow key-up activation only when there was no observed key-down
            // for this chord (some event taps report key-up-only for certain combos).
            if !is_down && had_observed_down {
                return ProcessResult::keep();
            }

            debug!("Entering layer: {:?}", hit.full_path);

            // Entering a child layer counts as handled activity in the parent layer.
            if depth > 0 {
                self.reset_top_deadline();
            }

            let child = ActiveLayer {
                name: layer_behavior.name.clone(),
                context: hit.context,
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
            context: registration.context.clone(),
            path: registration.path,
            deadline: Self::deadline_from_timeout(registration.behavior.timeout_ms),
            behavior: registration.behavior,
        });
        Ok(())
    }

    pub fn clear_active_layers(&mut self) {
        self.active_layers.clear();
    }

    pub fn pop_active_layer(&mut self) -> bool {
        self.active_layers.pop().is_some()
    }

    pub fn resolve_layer_target_name(
        &self,
        target: &str,
        app_scope: Option<&str>,
    ) -> Result<String, String> {
        if let Some(global_target) = target.strip_prefix("root.") {
            if global_target.is_empty() {
                return Err("layer target after 'root.' cannot be empty".to_string());
            }
            return self
                .match_layer_target(global_target, None)?
                .ok_or_else(|| format!("unknown global layer target: {global_target}"));
        }

        if let Some(app) = app_scope {
            if let Some(app_hit) = self.match_layer_target(target, Some(app))? {
                return Ok(app_hit);
            }
            return self
                .match_layer_target(target, None)?
                .ok_or_else(|| format!("unknown layer target '{target}' in app '{app}'"));
        }

        self.match_layer_target(target, None)?
            .ok_or_else(|| format!("unknown global layer target: {target}"))
    }

    fn match_layer_target(
        &self,
        target: &str,
        app_scope: Option<&str>,
    ) -> Result<Option<String>, String> {
        let mut hits = Vec::new();

        for name in self.layer_registry.keys() {
            let candidate = match app_scope {
                Some(app) => {
                    let prefix = format!("app:{app}.");
                    let Some(local) = name.strip_prefix(&prefix) else {
                        continue;
                    };
                    local
                }
                None => {
                    if !self
                        .layer_registry
                        .get(name)
                        .is_some_and(|reg| reg.context.is_none())
                    {
                        continue;
                    }
                    name.as_str()
                }
            };

            let matched = if target.contains('.') {
                candidate == target
            } else {
                candidate.rsplit('.').next() == Some(target)
            };

            if matched {
                hits.push(name.clone());
            }
        }

        if hits.is_empty() {
            return Ok(None);
        }
        if hits.len() > 1 {
            hits.sort();
            return Err(format!(
                "ambiguous layer target '{target}', matches: {}",
                hits.join(", ")
            ));
        }

        Ok(hits.into_iter().next())
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
    fn nested_miss_exits_layers_when_no_ancestor_matches() {
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
        assert!(!parent_hit.handled);
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

    #[test]
    fn app_layer_activation_shadows_global_only_in_that_app() {
        let mut mgr = HotkeyManager::new();
        mgr.register_layer(
            vec![step('a')],
            None,
            LayerBehavior {
                name: Some("global".to_string()),
                mode: LayerMode::Sticky,
                timeout_ms: None,
                deactivate: None,
            },
        );
        mgr.bind(vec![step('a'), step('x')], None, Action::Reload);

        mgr.register_layer(
            vec![step('a')],
            Some("Chrome".to_string()),
            LayerBehavior {
                name: Some("app".to_string()),
                mode: LayerMode::Sticky,
                timeout_ms: None,
                deactivate: None,
            },
        );
        mgr.bind(
            vec![step('a'), step('x')],
            Some("Chrome".to_string()),
            Action::Quit,
        );

        assert!(
            mgr.process(Key::Char('a'), Modifiers::NONE, true, "Chrome")
                .handled
        );
        let app_hit = mgr.process(Key::Char('x'), Modifiers::NONE, true, "Chrome");
        assert!(matches!(app_hit.action, Some(Action::Quit)));

        mgr.clear_active_layers();
        assert!(
            mgr.process(Key::Char('a'), Modifiers::NONE, true, "Terminal")
                .handled
        );
        let global_hit = mgr.process(Key::Char('x'), Modifiers::NONE, true, "Terminal");
        assert!(matches!(global_hit.action, Some(Action::Reload)));
    }

    #[test]
    fn app_binding_shadows_global_layer_only_in_that_app() {
        let mut mgr = HotkeyManager::new();
        mgr.register_layer(
            vec![step('a')],
            None,
            LayerBehavior {
                name: Some("global".to_string()),
                mode: LayerMode::Sticky,
                timeout_ms: None,
                deactivate: None,
            },
        );
        mgr.bind(vec![step('a'), step('x')], None, Action::Reload);
        mgr.bind(vec![step('a')], Some("Chrome".to_string()), Action::Quit);

        let app_hit = mgr.process(Key::Char('a'), Modifiers::NONE, true, "Chrome");
        assert!(matches!(app_hit.action, Some(Action::Quit)));

        mgr.clear_active_layers();
        assert!(
            mgr.process(Key::Char('a'), Modifiers::NONE, true, "Terminal")
                .handled
        );
        let global_hit = mgr.process(Key::Char('x'), Modifiers::NONE, true, "Terminal");
        assert!(matches!(global_hit.action, Some(Action::Reload)));
    }

    #[test]
    fn repeated_activation_key_does_not_reactivate_on_key_up() {
        let mut mgr = HotkeyManager::new();
        mgr.register_layer(vec![step('l')], None, oneshot(None));

        let first_down = mgr.process(Key::Char('l'), Modifiers::NONE, true, "");
        assert!(first_down.handled);
        let first_up = mgr.process(Key::Char('l'), Modifiers::NONE, false, "");
        assert!(first_up.handled);

        let second_down = mgr.process(Key::Char('l'), Modifiers::NONE, true, "");
        assert!(second_down.handled);
        let second_up = mgr.process(Key::Char('l'), Modifiers::NONE, false, "");
        assert!(second_up.handled);
    }

    #[test]
    fn keyup_only_can_still_activate_layer() {
        let mut mgr = HotkeyManager::new();
        mgr.register_layer(vec![step('l')], None, oneshot(None));
        mgr.bind(vec![step('l'), step('x')], None, Action::Reload);

        let up_only = mgr.process(Key::Char('l'), Modifiers::NONE, false, "");
        assert!(up_only.handled);
        let hit = mgr.process(Key::Char('x'), Modifiers::NONE, true, "");
        assert!(matches!(hit.action, Some(Action::Reload)));
    }

    #[test]
    fn app_switch_purges_app_scoped_active_layer() {
        let mut mgr = HotkeyManager::new();
        mgr.register_layer(
            vec![step('t')],
            Some("Google Chrome".to_string()),
            LayerBehavior {
                name: Some("tabs".to_string()),
                mode: LayerMode::Sticky,
                timeout_ms: None,
                deactivate: None,
            },
        );

        assert!(
            mgr.process(Key::Char('t'), Modifiers::NONE, true, "Google Chrome")
                .handled
        );
        assert_eq!(mgr.active_layer_names(), vec!["tabs".to_string()]);

        let switched = mgr.process(Key::Char('z'), Modifiers::NONE, true, "Ghostty");
        assert!(!switched.handled);
        assert!(mgr.active_layer_names().is_empty());
    }

    #[test]
    fn keydown_miss_falls_back_to_root_in_same_event() {
        let mut mgr = HotkeyManager::new();
        mgr.register_layer(
            vec![step('t')],
            Some("Google Chrome".to_string()),
            LayerBehavior {
                name: Some("tabs".to_string()),
                mode: LayerMode::Sticky,
                timeout_ms: None,
                deactivate: None,
            },
        );
        mgr.register_layer(
            vec![step('l')],
            None,
            LayerBehavior {
                name: Some("launch".to_string()),
                mode: LayerMode::Sticky,
                timeout_ms: None,
                deactivate: None,
            },
        );

        assert!(
            mgr.process(Key::Char('t'), Modifiers::NONE, true, "Google Chrome")
                .handled
        );
        let activate_launch = mgr.process(Key::Char('l'), Modifiers::NONE, true, "Google Chrome");
        assert!(activate_launch.handled);
        assert_eq!(mgr.active_layer_names(), vec!["launch".to_string()]);
    }

    #[test]
    fn keyup_miss_does_not_fallback_pop_or_activate() {
        let mut mgr = HotkeyManager::new();
        mgr.register_layer(vec![step('a')], None, sticky(None));
        mgr.bind(vec![step('a'), step('x')], None, Action::Reload);

        assert!(
            mgr.process(Key::Char('a'), Modifiers::NONE, true, "")
                .handled
        );
        let miss_up = mgr.process(Key::Char('z'), Modifiers::NONE, false, "");
        assert!(!miss_up.handled);

        let still_active = mgr.process(Key::Char('x'), Modifiers::NONE, true, "");
        assert!(matches!(still_active.action, Some(Action::Reload)));
    }

    #[test]
    fn keydown_miss_pops_multiple_layers_until_root_hit() {
        let mut mgr = HotkeyManager::new();
        mgr.register_layer(vec![step('a')], None, sticky(None));
        mgr.register_layer(vec![step('a'), step('b')], None, sticky(None));
        mgr.bind(vec![step('l')], None, Action::Reload);

        assert!(
            mgr.process(Key::Char('a'), Modifiers::NONE, true, "")
                .handled
        );
        assert!(
            mgr.process(Key::Char('b'), Modifiers::NONE, true, "")
                .handled
        );

        let root_hit = mgr.process(Key::Char('l'), Modifiers::NONE, true, "");
        assert!(matches!(root_hit.action, Some(Action::Reload)));
        assert!(mgr.active_layer_names().is_empty());
    }

    #[test]
    fn resolve_layer_target_prefers_app_local_then_global() {
        let mut mgr = HotkeyManager::new();
        mgr.register_layer(
            vec![step('l')],
            None,
            LayerBehavior {
                name: Some("launch".to_string()),
                mode: LayerMode::Sticky,
                timeout_ms: None,
                deactivate: None,
            },
        );
        mgr.register_layer(
            vec![step('t')],
            Some("Google Chrome".to_string()),
            LayerBehavior {
                name: Some("app:Google Chrome.launch".to_string()),
                mode: LayerMode::Sticky,
                timeout_ms: None,
                deactivate: None,
            },
        );

        let local = mgr
            .resolve_layer_target_name("launch", Some("Google Chrome"))
            .expect("app-local should resolve");
        assert_eq!(local, "app:Google Chrome.launch");

        let forced_global = mgr
            .resolve_layer_target_name("root.launch", Some("Google Chrome"))
            .expect("global should resolve");
        assert_eq!(forced_global, "launch");
    }

    #[test]
    fn resolve_layer_target_global_scope_cannot_see_app_layers() {
        let mut mgr = HotkeyManager::new();
        mgr.register_layer(
            vec![step('t')],
            Some("Google Chrome".to_string()),
            LayerBehavior {
                name: Some("app:Google Chrome.tabs".to_string()),
                mode: LayerMode::Sticky,
                timeout_ms: None,
                deactivate: None,
            },
        );

        let err = mgr
            .resolve_layer_target_name("tabs", None)
            .expect_err("must fail");
        assert!(err.contains("unknown global layer target"));
    }
}
