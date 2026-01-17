use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use crate::parser::{Key, Modifiers};
use std::hash::Hash;


#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HotkeyStep {
    pub key: Key,
    pub modifiers: Modifiers,
}

impl HotkeyStep {
    pub fn new(key: Key, modifiers: Modifiers) -> Self {
        Self { key, modifiers }
    }
}

pub type Handler = Arc<dyn Fn() + Send + Sync>;

#[derive(Clone)]
pub struct HotkeyNode {
    pub handler: Option<Handler>,
    pub context: Option<String>, // New field for app context
    pub children: HashMap<HotkeyStep, HotkeyNode>,
}

impl HotkeyNode {
    pub fn new() -> Self {
        Self {
            handler: None,
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

impl HotkeyManager {
    pub fn new() -> Self {
        Self {
            root: HotkeyNode::new(),
            current_path: Vec::new(),
            active_activations: HashSet::new(),
        }
    }

    pub fn bind<F>(&mut self, sequence: Vec<HotkeyStep>, context: Option<String>, handler: F)
    where
        F: Fn() + Send + Sync + 'static,
    {
        let mut node = &mut self.root;
        for step in sequence {
            node = node.children.entry(step).or_insert_with(HotkeyNode::new);
        }
        node.handler = Some(Arc::new(handler));
        node.context = context;
    }

    pub fn process(&mut self, key: Key, modifiers: Modifiers, is_down: bool, current_app: &str) -> bool {
        let step = HotkeyStep { key, modifiers };
        
        // --- RELEASE HANDLING (KeyUp) ---
        if !is_down {
            // Check if this specific step sequence was marked as active
            // We need to approximate the sequence. Since 'step' is just one key,
            // we check if ANY active activation ends with this step.
            // Simplified: If we are in a path, we are checking that path.
            // But if KeyDown was missed, we might not be in a path?
            // Actually, for single binds like Hyper+Esc, the path is size 1.
            // Let's iterate `active_activations` to see if we should remove one.
            
            // For Kanata Compatibility:
            // If we receive a KeyUp that matches a valid bind, AND it wasn't tracked as active,
            // it means we missed the KeyDown. We should Execute it now.
            
            // 1. Try to match this step against root (single hotkeys)
            if let Some(node) = self.root.children.get(&step) {
                if let Some(handler) = &node.handler {
                    // Check context
                    let valid_context = node.context.as_deref() == Some(current_app) || node.context.is_none();
                    if valid_context {
                        let seq = vec![step.clone()];
                        if self.active_activations.contains(&seq) {
                            // Normal case: KeyDown happened, now KeyUp. Just cleanup.
                            self.active_activations.remove(&seq);
                            return true; // Consume the KeyUp
                        } else {
                            // Kanata case: KeyDown missed. Execute now!
                            handler();
                            return true;
                        }
                    }
                }
            }
            
            return false;
        }

        // --- PRESS HANDLING (KeyDown) ---

        // Try to descend from current path
        let mut node = &self.root;
        for s in &self.current_path {
            if let Some(next) = node.children.get(s) {
                node = next;
            } else {
                self.current_path.clear();
                return false;
            }
        }

        if let Some(next_node) = node.children.get(&step) {
            // Check context if it's a leaf node (handler present)
            if let Some(handler) = &next_node.handler {
                if let Some(ctx) = &next_node.context {
                    if ctx != current_app {
                        self.current_path.clear();
                        return false;
                    }
                }
                
                // EXECUTE
                handler();
                
                // Track activation
                let mut full_seq = self.current_path.clone();
                full_seq.push(step);
                self.active_activations.insert(full_seq);

                self.current_path.clear();
                return true;
            } else {
                // Not a leaf, just descend
                self.current_path.push(step);
                return true;
            }
        } else {
            self.current_path.clear();

            // Try matching as a start of a new sequence
            if let Some(start_node) = self.root.children.get(&step) {
                if let Some(handler) = &start_node.handler {
                    if let Some(ctx) = &start_node.context {
                        if ctx != current_app {
                            return false;
                        }
                    }
                    
                    // EXECUTE
                    handler();

                    // Track activation
                    let seq = vec![step];
                    self.active_activations.insert(seq);

                    return true;
                } else {
                    self.current_path.push(step);
                    return true;
                }
            }
        }

        false
    }
}
