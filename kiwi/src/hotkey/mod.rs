use kiwi_parser::{Action, Key, Modifiers};
use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use tracing::{debug, trace};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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

impl HotkeyStep {
    pub fn new(key: Key, modifiers: Modifiers) -> Self {
        Self { key, modifiers }
    }
}

#[derive(Clone)]
pub struct HotkeyNode {
    pub action: Option<Action>,
    pub context: Option<String>, // New field for app context
    pub children: HashMap<HotkeyStep, HotkeyNode>,
}

impl HotkeyNode {
    pub fn new() -> Self {
        Self {
            action: None,
            context: None,
            children: HashMap::new(),
        }
    }
}

#[derive(Clone)]
pub struct HotkeyManager {
    root: HotkeyNode,
    current_path: Vec<HotkeyStep>,
    active_activations: HashSet<Vec<HotkeyStep>>,
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
            current_path: Vec::new(),
            active_activations: HashSet::new(),
        }
    }

    pub fn bind(&mut self, sequence: Vec<HotkeyStep>, context: Option<String>, action: Action) {
        let mut node = &mut self.root;
        for step in sequence {
            node = node.children.entry(step).or_insert_with(HotkeyNode::new);
        }
        node.action = Some(action);
        node.context = context;
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

        // --- RELEASE HANDLING (KeyUp) ---
        if !is_down {
            // If we receive a KeyUp that matches a valid bind, AND it wasn't tracked as active,
            // it means we missed the KeyDown. We should Execute it now.

            // 1. Try to match this step against root (single hotkeys)
            if let Some(node) = self.root.children.get(&step) {
                if let Some(action) = &node.action {
                    // Check context
                    let valid_context =
                        node.context.as_deref() == Some(current_app) || node.context.is_none();
                    if valid_context {
                        let seq = vec![step.clone()];
                        if self.active_activations.contains(&seq) {
                            // Normal case: KeyDown happened, now KeyUp. Just cleanup.
                            self.active_activations.remove(&seq);
                            return ProcessResult::consume(None); // Consume the KeyUp
                        } else {
                            // KeyDown missed
                            debug!("Executing on KeyUp for {step}");
                            return ProcessResult::consume(Some(action.clone()));
                        }
                    }
                }
            }

            return ProcessResult::keep();
        }

        // --- PRESS HANDLING (KeyDown) ---

        // Try to descend from current path
        let mut node = &self.root;
        for s in &self.current_path {
            if let Some(next) = node.children.get(s) {
                node = next;
            } else {
                self.current_path.clear();
                return ProcessResult::keep();
            }
        }

        if let Some(next_node) = node.children.get(&step) {
            // Check context if it's a leaf node (handler present)
            if let Some(action) = &next_node.action {
                if let Some(ctx) = &next_node.context {
                    if ctx != current_app {
                        self.current_path.clear();
                        return ProcessResult::keep();
                    }
                }

                // EXECUTE
                debug!(
                    "Executing hotkey sequence: {:?}",
                    self.current_path
                        .iter()
                        .chain(std::iter::once(&step))
                        .map(|s| s.to_string())
                        .collect::<Vec<_>>()
                );
                // Track activation
                let mut full_seq = self.current_path.clone();
                full_seq.push(step);
                self.active_activations.insert(full_seq);

                self.current_path.clear();
                return ProcessResult::consume(Some(action.clone()));
            } else {
                // Not a leaf, just descend
                debug!(
                    "Entering layer: {:?}",
                    self.current_path
                        .iter()
                        .chain(std::iter::once(&step))
                        .map(|s| s.to_string())
                        .collect::<Vec<_>>()
                );
                self.current_path.push(step);
                return ProcessResult::consume(None);
            }
        } else {
            self.current_path.clear();

            // Try matching as a start of a new sequence
            if let Some(start_node) = self.root.children.get(&step) {
                if let Some(action) = &start_node.action {
                    if let Some(ctx) = &start_node.context {
                        if ctx != current_app {
                            return ProcessResult::keep();
                        }
                    }

                    // EXECUTE
                    debug!("Executing hotkey: {step:?}");
                    // Track activation
                    let seq = vec![step];
                    self.active_activations.insert(seq);

                    return ProcessResult::consume(Some(action.clone()));
                } else {
                    debug!("Starting layer sequence: {step:?}");
                    self.current_path.push(step);
                    return ProcessResult::consume(None);
                }
            }
        }

        ProcessResult::keep()
    }
}
