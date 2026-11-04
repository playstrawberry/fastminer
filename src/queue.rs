use std::collections::{HashMap, VecDeque};

pub type Owner = (u64, u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ticket(pub Owner, pub u64);

pub struct Queue<T> {
    pub items: VecDeque<(Ticket, T)>,
    pub active: HashMap<Owner, u64>,
    pub scheduled: bool,
    pub closed: bool,
    pub synthetic: Option<(Owner, crate::walk::Pos)>,
    next: u64,
}

impl<T> Default for Queue<T> {
    fn default() -> Self {
        Self {
            items: VecDeque::new(),
            active: HashMap::new(),
            scheduled: false,
            closed: false,
            synthetic: None,
            next: 0,
        }
    }
}

impl<T> Queue<T> {
    pub fn insert(&mut self, owner: Owner, value: T, cap: usize) -> Option<Ticket> {
        if self.closed || self.active.len() >= cap.min(16) || self.active.contains_key(&owner) {
            return None;
        }
        self.next = self.next.checked_add(1)?;
        let ticket = Ticket(owner, self.next);
        self.active.insert(owner, self.next);
        self.items.push_back((ticket, value));
        Some(ticket)
    }
    pub fn live(&self, ticket: Ticket) -> bool {
        !self.closed && self.active.get(&ticket.0) == Some(&ticket.1)
    }
    pub fn finish(&mut self, ticket: Ticket) {
        if self.live(ticket) {
            self.active.remove(&ticket.0);
        }
    }
    pub fn cancel(&mut self, owner: Owner) {
        self.active.remove(&owner);
        self.items.retain(|(ticket, _)| ticket.0 != owner);
    }
    pub fn close(&mut self) {
        self.closed = true;
        self.active.clear();
        self.items.clear();
        self.synthetic = None;
        self.scheduled = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn queue_is_bounded_and_one_vein_per_player() {
        let mut q = Queue::default();
        assert!(q.insert((1, 0), 1, 2).is_some());
        assert!(q.insert((1, 0), 2, 2).is_none());
        assert!(q.insert((2, 0), 3, 2).is_some());
        assert!(q.insert((3, 0), 4, 2).is_none());
        assert_eq!(q.items.len(), 2);
    }
    #[test]
    fn disconnect_and_rejoin_cannot_revive_old_work() {
        let mut q = Queue::default();
        let old = q.insert((1, 0), 1, 2).unwrap();
        q.items.pop_front();
        q.cancel((1, 0));
        let new = q.insert((1, 0), 2, 2).unwrap();
        assert!(!q.live(old));
        q.finish(old);
        assert!(q.live(new));
        assert_eq!(q.items.len(), 1);
        q.finish(new);
        assert!(!q.live(new));
    }
    #[test]
    fn unload_releases_queue_and_refuses_new_work() {
        let mut q = Queue::default();
        q.insert((1, 0), 1, 2).unwrap();
        q.scheduled = true;
        q.synthetic = Some(((1, 0), [0, 0, 0]));
        q.close();
        assert!(q.items.is_empty() && q.active.is_empty());
        assert!(!q.scheduled && q.synthetic.is_none());
        assert!(q.insert((2, 0), 2, 2).is_none());
    }
}
