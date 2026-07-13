use std::collections::BTreeMap;

use crate::ActionResult;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PopoverCoordinator {
    active_id: Option<String>,
    errors: BTreeMap<String, String>,
}

impl PopoverCoordinator {
    pub fn open(&mut self, id: &str) -> Option<String> {
        let next = id.to_string();
        let previous = self.active_id.replace(next.clone());
        if previous.as_deref() == Some(id) {
            None
        } else {
            previous
        }
    }

    pub fn close(&mut self, id: &str) {
        if self.active_id.as_deref() == Some(id) {
            self.active_id = None;
        }
        self.errors.remove(id);
        if let Some(system_id) = id.strip_prefix("system:") {
            self.errors.remove(system_id);
        }
    }

    pub fn clear_active(&mut self) -> Option<String> {
        let previous = self.active_id.take();
        if let Some(id) = previous.as_ref() {
            self.errors.remove(id);
            if let Some(system_id) = id.strip_prefix("system:") {
                self.errors.remove(system_id);
            }
        }
        previous
    }

    pub fn active_id(&self) -> Option<&str> {
        self.active_id.as_deref()
    }

    pub fn before_action(&mut self, origin: &str) {
        if let Some(id) = popover_id_from_origin(origin) {
            self.errors.remove(&id);
        }
    }

    pub fn record_completion(&mut self, origin: &str, result: &ActionResult) {
        let Some(id) = popover_id_from_origin(origin) else {
            return;
        };

        if let ActionResult::Failed { detail, .. } = result {
            self.errors.insert(id, detail.clone());
        }
    }

    pub fn error_for(&self, id: &str) -> Option<&str> {
        self.errors.get(id).map(String::as_str)
    }
}

pub fn popover_id_from_origin(origin: &str) -> Option<String> {
    origin
        .strip_prefix("system-popover:")?
        .split(':')
        .next()
        .filter(|segment| !segment.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use crate::ActionResult;

    use super::PopoverCoordinator;

    #[test]
    fn opening_one_popover_closes_the_previous_and_escape_clears_active_id() {
        let mut coordinator = PopoverCoordinator::default();

        assert_eq!(coordinator.open("keyboard"), None);
        assert_eq!(coordinator.active_id(), Some("keyboard"));

        assert_eq!(coordinator.open("power"), Some("keyboard".to_string()));
        assert_eq!(coordinator.active_id(), Some("power"));

        assert_eq!(coordinator.clear_active(), Some("power".to_string()));
        assert_eq!(coordinator.active_id(), None);
    }

    #[test]
    fn failures_stay_with_the_originating_popover_until_next_action_or_close() {
        let mut coordinator = PopoverCoordinator::default();
        coordinator.open("power");

        coordinator.record_completion(
            "system-popover:power:cycle-next",
            &ActionResult::Failed {
                summary: "Action failed".to_string(),
                detail: "powerprofilesctl missing".to_string(),
            },
        );
        assert_eq!(
            coordinator.error_for("power"),
            Some("powerprofilesctl missing")
        );

        coordinator.before_action("system-popover:power:cycle-prev");
        assert_eq!(coordinator.error_for("power"), None);

        coordinator.record_completion(
            "system-popover:power:cycle-prev",
            &ActionResult::Failed {
                summary: "Action failed".to_string(),
                detail: "permission denied".to_string(),
            },
        );
        coordinator.close("power");
        assert_eq!(coordinator.active_id(), None);
        assert_eq!(coordinator.error_for("power"), None);
    }
}
