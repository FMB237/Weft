//! Basic plugin architecture.

use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum PluginEvent {
    VaultOpened(PathBuf),
    DocumentSaved(PathBuf),
    DocumentOpened(PathBuf),
}

pub trait Plugin: std::fmt::Debug + Send + Sync {
    fn name(&self) -> &str;
    fn on_load(&mut self, _manager: &mut PluginManager) -> Result<()> {
        Ok(())
    }
    fn on_event(&mut self, _event: &PluginEvent, _manager: &mut PluginManager) -> Result<()> {
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct PluginManager {
    plugins: HashMap<String, Box<dyn Plugin>>,
    order: Vec<String>,
}

impl PluginManager {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn register(&mut self, mut plugin: Box<dyn Plugin>) -> Result<()> {
        let name = plugin.name().to_string();
        if self.plugins.contains_key(&name) {
            anyhow::bail!("plugin '{name}' already registered");
        }
        plugin.on_load(self)?;
        self.plugins.insert(name.clone(), plugin);
        self.order.push(name);
        Ok(())
    }
    pub fn dispatch(&mut self, event: &PluginEvent) -> Result<()> {
        // Temporarily remove each plugin so we can pass `self` mutably to
        // `on_event` without a conflicting borrow of `self.plugins`.
        for name in self.order.clone() {
            if let Some(mut plugin) = self.plugins.remove(&name) {
                plugin.on_event(event, self)?;
                self.plugins.insert(name, plugin);
            }
        }
        Ok(())
    }
    pub fn plugin_count(&self) -> usize {
        self.plugins.len()
    }
    pub fn plugin_names(&self) -> Vec<&str> {
        self.order.iter().map(std::string::String::as_str).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct CounterPlugin {
        name: &'static str,
        vault_opens: std::sync::atomic::AtomicUsize,
    }

    impl CounterPlugin {
        fn new(name: &'static str) -> Self {
            Self {
                name,
                vault_opens: std::sync::atomic::AtomicUsize::new(0),
            }
        }
    }

    impl Plugin for CounterPlugin {
        fn name(&self) -> &str {
            self.name
        }
        fn on_event(&mut self, event: &PluginEvent, _m: &mut PluginManager) -> Result<()> {
            if matches!(event, PluginEvent::VaultOpened(_)) {
                self.vault_opens
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            Ok(())
        }
    }

    #[test]
    fn plugin_register_and_dispatch() {
        let mut mgr = PluginManager::new();
        mgr.register(Box::new(CounterPlugin::new("a"))).unwrap();
        mgr.register(Box::new(CounterPlugin::new("b"))).unwrap();
        assert_eq!(mgr.plugin_count(), 2);
        mgr.dispatch(&PluginEvent::VaultOpened(PathBuf::from("/v")))
            .unwrap();
        assert_eq!(mgr.plugin_names(), vec!["a", "b"]);
    }

    #[test]
    fn duplicate_plugin_name_rejected() {
        let mut mgr = PluginManager::new();
        mgr.register(Box::new(CounterPlugin::new("dup"))).unwrap();
        assert!(mgr.register(Box::new(CounterPlugin::new("dup"))).is_err());
    }
}
