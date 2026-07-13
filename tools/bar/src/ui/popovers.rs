#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PopoverCoordinator {
    active_id: Option<String>,
}

impl PopoverCoordinator {
    pub fn open(&mut self, id: &str) -> Option<String> {
        let next = id.to_string();
        let previous = self.active_id.replace(next);
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
    }

    pub fn clear_active(&mut self) -> Option<String> {
        self.active_id.take()
    }

    #[cfg(test)]
    pub fn active_id(&self) -> Option<&str> {
        self.active_id.as_deref()
    }
}

#[cfg(test)]
mod tests {
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
}
