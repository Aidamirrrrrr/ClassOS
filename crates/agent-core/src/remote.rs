//! Независимая от Windows state machine эксклюзивного remote control T4.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteControlState {
    Idle,
    Active {
        owner_id: String,
        session_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteControlError {
    OwnedByAnotherTeacher,
    NotOwner,
    NotActive,
}

#[derive(Debug, Default)]
pub struct RemoteControlSession {
    state: Option<(String, String)>,
}

impl RemoteControlSession {
    pub fn start(
        &mut self,
        owner_id: String,
        session_id: String,
    ) -> Result<(), RemoteControlError> {
        match &self.state {
            Some((existing, _)) if existing != &owner_id => {
                Err(RemoteControlError::OwnedByAnotherTeacher)
            }
            Some(_) => Ok(()),
            None => {
                self.state = Some((owner_id, session_id));
                Ok(())
            }
        }
    }

    pub fn stop(&mut self, owner_id: &str) -> Result<String, RemoteControlError> {
        match &self.state {
            Some((existing, _)) if existing == owner_id => {
                Ok(self.state.take().expect("state checked").1)
            }
            Some(_) => Err(RemoteControlError::NotOwner),
            None => Err(RemoteControlError::NotActive),
        }
    }

    pub fn disconnect(&mut self) -> Option<String> {
        self.state.take().map(|(_, session_id)| session_id)
    }

    pub fn state(&self) -> RemoteControlState {
        match &self.state {
            Some((owner_id, session_id)) => RemoteControlState::Active {
                owner_id: owner_id.clone(),
                session_id: session_id.clone(),
            },
            None => RemoteControlState::Idle,
        }
    }

    pub fn accepts_input(&self, owner_id: &str) -> bool {
        self.state
            .as_ref()
            .is_some_and(|(owner, _)| owner == owner_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_one_teacher_can_control_device() {
        let mut session = RemoteControlSession::default();
        session
            .start("teacher-a".to_owned(), "session-a".to_owned())
            .unwrap();
        assert_eq!(
            session.start("teacher-b".to_owned(), "session-b".to_owned()),
            Err(RemoteControlError::OwnedByAnotherTeacher)
        );
        assert!(session.accepts_input("teacher-a"));
        assert!(!session.accepts_input("teacher-b"));
    }

    #[test]
    fn disconnect_always_releases_owner() {
        let mut session = RemoteControlSession::default();
        session
            .start("teacher-a".to_owned(), "session-a".to_owned())
            .unwrap();
        assert_eq!(session.disconnect(), Some("session-a".to_owned()));
        assert_eq!(session.state(), RemoteControlState::Idle);
    }
}
