use parking_lot::RwLock;
use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use thiserror::Error;

/// A unique identifier for a transaction.
pub type TransactionId = u64;

pub const INVALID_TXN_ID: TransactionId = 0;
pub const FIRST_NORMAL_TXN_ID: TransactionId = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionState {
    Active,
    Committed,
    Aborted,
}

#[derive(Error, Debug)]
pub enum TransactionError {
    #[error("Transaction {0} not found")]
    NotFound(TransactionId),
}

/// The TransactionManager tracks active, committed, and aborted transactions.
/// It provides XIDs and logic to determine MVCC visibility.
pub struct TransactionManager {
    next_txn_id: AtomicU64,
    active_txns: RwLock<HashSet<TransactionId>>,
    committed_txns: RwLock<HashSet<TransactionId>>,
    aborted_txns: RwLock<HashSet<TransactionId>>,
}

impl Default for TransactionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TransactionManager {
    pub fn new() -> Self {
        Self {
            next_txn_id: AtomicU64::new(FIRST_NORMAL_TXN_ID),
            active_txns: RwLock::new(HashSet::new()),
            committed_txns: RwLock::new(HashSet::new()),
            aborted_txns: RwLock::new(HashSet::new()),
        }
    }

    /// Begins a new transaction, assigns it an XID, and records it as active.
    pub fn begin(&self) -> TransactionId {
        let txn_id = self.next_txn_id.fetch_add(1, Ordering::SeqCst);
        let mut active = self.active_txns.write();
        active.insert(txn_id);
        txn_id
    }

    /// Commits an active transaction.
    pub fn commit(&self, txn_id: TransactionId) -> Result<(), TransactionError> {
        let mut active = self.active_txns.write();
        if !active.remove(&txn_id) {
            return Err(TransactionError::NotFound(txn_id));
        }
        let mut committed = self.committed_txns.write();
        committed.insert(txn_id);
        Ok(())
    }

    /// Aborts an active transaction.
    pub fn abort(&self, txn_id: TransactionId) -> Result<(), TransactionError> {
        let mut active = self.active_txns.write();
        if !active.remove(&txn_id) {
            return Err(TransactionError::NotFound(txn_id));
        }
        let mut aborted = self.aborted_txns.write();
        aborted.insert(txn_id);
        Ok(())
    }

    /// Checks the state of a transaction.
    pub fn get_state(&self, txn_id: TransactionId) -> TransactionState {
        if self.committed_txns.read().contains(&txn_id) {
            TransactionState::Committed
        } else if self.aborted_txns.read().contains(&txn_id) {
            TransactionState::Aborted
        } else {
            TransactionState::Active
        }
    }

    /// Determines if a tuple with the given xmin/xmax is visible to the current transaction.
    /// Basic MVCC Snapshot Isolation Logic.
    pub fn is_visible(&self, xmin: TransactionId, xmax: TransactionId, current_txn: TransactionId) -> bool {
        // A tuple is NOT visible if the creator (xmin) hasn't committed
        // and is not the current transaction.
        if xmin != current_txn && self.get_state(xmin) != TransactionState::Committed {
            return false;
        }

        // A tuple is NOT visible if it has been deleted (xmax valid),
        // and the deleter has committed, or the deleter IS the current transaction.
        if xmax != INVALID_TXN_ID {
            if xmax == current_txn || self.get_state(xmax) == TransactionState::Committed {
                return false;
            }
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn begin_should_assign_monotonically_increasing_xids() {
        // Arrange
        let tm = TransactionManager::new();
        
        // Act
        let t1 = tm.begin();
        let t2 = tm.begin();

        // Assert
        assert_eq!(t1, FIRST_NORMAL_TXN_ID);
        assert_eq!(t2, FIRST_NORMAL_TXN_ID + 1);
        assert_eq!(tm.get_state(t1), TransactionState::Active);
    }

    #[test]
    fn commit_should_make_transaction_committed() {
        // Arrange
        let tm = TransactionManager::new();
        let t1 = tm.begin();

        // Act
        tm.commit(t1).unwrap();

        // Assert
        assert_eq!(tm.get_state(t1), TransactionState::Committed);
    }

    #[test]
    fn is_visible_should_hide_uncommitted_inserts() {
        // Arrange
        let tm = TransactionManager::new();
        let t_creator = tm.begin();
        let t_reader = tm.begin();

        // Act
        let visible = tm.is_visible(t_creator, INVALID_TXN_ID, t_reader);

        // Assert
        assert!(!visible);
    }

    #[test]
    fn is_visible_should_show_committed_inserts() {
        // Arrange
        let tm = TransactionManager::new();
        let t_creator = tm.begin();
        let t_reader = tm.begin();
        tm.commit(t_creator).unwrap();

        // Act
        let visible = tm.is_visible(t_creator, INVALID_TXN_ID, t_reader);

        // Assert
        assert!(visible);
    }

    #[test]
    fn is_visible_should_hide_committed_deletes() {
        // Arrange
        let tm = TransactionManager::new();
        let t_creator = tm.begin();
        let t_deleter = tm.begin();
        let t_reader = tm.begin();
        
        tm.commit(t_creator).unwrap();
        tm.commit(t_deleter).unwrap();

        // Act
        let visible = tm.is_visible(t_creator, t_deleter, t_reader);

        // Assert
        assert!(!visible);
    }
}
