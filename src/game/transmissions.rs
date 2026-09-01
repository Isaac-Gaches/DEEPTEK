use std::collections::VecDeque;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Transmission {
    sequence: u64,
    sender: String,
    subject: String,
    body: String,
}

impl Transmission {
    pub fn new(
        sender: impl Into<String>,
        subject: impl Into<String>,
        body: impl Into<String>,
    ) -> Self {
        Self {
            sequence: 0,
            sender: sender.into(),
            subject: subject.into(),
            body: body.into(),
        }
    }

    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    pub fn sender(&self) -> &str {
        &self.sender
    }

    pub fn subject(&self) -> &str {
        &self.subject
    }

    pub fn body(&self) -> &str {
        &self.body
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TransmissionLog {
    history: Vec<Transmission>,
    incoming: Option<usize>,
    queued: VecDeque<usize>,
}

impl TransmissionLog {
    pub fn receive(&mut self, mut transmission: Transmission) {
        transmission.sequence = self.history.len() as u64 + 1;
        self.history.push(transmission);
        let index = self.history.len() - 1;
        if self.incoming.is_none() {
            self.show(index);
        } else {
            self.queued.push_back(index);
        }
    }

    pub fn history(&self) -> &[Transmission] {
        &self.history
    }

    pub fn incoming(&self) -> Option<&Transmission> {
        self.incoming.and_then(|index| self.history.get(index))
    }

    pub fn dismiss_incoming(&mut self) -> bool {
        if self.incoming.is_none() {
            return false;
        }
        if let Some(next) = self.queued.pop_front() {
            self.show(next);
        } else {
            self.incoming = None;
        }
        true
    }

    pub(crate) fn saved_indices(&self) -> (Option<usize>, impl Iterator<Item = usize> + '_) {
        (self.incoming, self.queued.iter().copied())
    }

    pub(crate) fn from_saved(
        history: Vec<Transmission>,
        incoming: Option<usize>,
        queued: VecDeque<usize>,
    ) -> Option<Self> {
        let indices_are_valid = incoming
            .into_iter()
            .chain(queued.iter().copied())
            .all(|index| index < history.len());
        let queued_indices: Vec<_> = queued.iter().copied().collect();
        let queue_is_valid = incoming.is_none_or(|incoming| !queued_indices.contains(&incoming))
            && !queued_indices
                .iter()
                .enumerate()
                .any(|(index, queued)| queued_indices[index + 1..].contains(queued));
        let sequences_are_valid = history
            .iter()
            .enumerate()
            .all(|(index, transmission)| transmission.sequence == index as u64 + 1);
        (indices_are_valid && queue_is_valid && sequences_are_valid).then_some(Self {
            history,
            incoming,
            queued,
        })
    }

    fn show(&mut self, index: usize) {
        self.incoming = Some(index);
    }
}

impl Transmission {
    pub(crate) fn from_saved(sequence: u64, sender: String, subject: String, body: String) -> Self {
        Self {
            sequence,
            sender,
            subject,
            body,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn transmission(subject: &str) -> Transmission {
        Transmission::new("DEEPTEK", subject, "BODY")
    }

    #[test]
    fn received_transmissions_are_logged_and_shown_in_order() {
        let mut log = TransmissionLog::default();
        log.receive(transmission("FIRST"));
        log.receive(transmission("SECOND"));

        assert_eq!(log.history().len(), 2);
        assert_eq!(log.history()[0].sequence(), 1);
        assert_eq!(log.incoming().unwrap().subject(), "FIRST");

        assert_eq!(log.incoming().unwrap().subject(), "FIRST");
        log.dismiss_incoming();
        assert_eq!(log.incoming().unwrap().subject(), "SECOND");
        log.dismiss_incoming();
        assert!(log.incoming().is_none());
        assert_eq!(log.history().len(), 2);
    }

    #[test]
    fn incoming_banner_remains_until_dismissed() {
        let mut log = TransmissionLog::default();
        log.receive(transmission("STABLE"));
        assert_eq!(log.incoming().unwrap().subject(), "STABLE");
    }

    #[test]
    fn dismissing_advances_the_incoming_queue_without_losing_history() {
        let mut log = TransmissionLog::default();
        log.receive(transmission("FIRST"));
        log.receive(transmission("SECOND"));

        assert!(log.dismiss_incoming());
        assert_eq!(log.incoming().unwrap().subject(), "SECOND");
        assert!(log.dismiss_incoming());
        assert!(log.incoming().is_none());
        assert!(!log.dismiss_incoming());
        assert_eq!(log.history().len(), 2);
    }
}
