//! Process-local, concurrent retirement-plan repository.

use application::{PlanRepository, RepositoryError, StoredPlan};
use domain::{Account, PlanId, PlanProfile};
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
    time::SystemTime,
};

pub type SharedPlanStore = Arc<RwLock<HashMap<PlanId, StoredPlan>>>;

#[derive(Clone, Debug, Default)]
pub struct MemoryPlanRepository {
    store: SharedPlanStore,
}

impl MemoryPlanRepository {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
    #[must_use]
    pub const fn from_store(store: SharedPlanStore) -> Self {
        Self { store }
    }
    #[must_use]
    pub fn shared_store(&self) -> SharedPlanStore {
        Arc::clone(&self.store)
    }
}

impl PlanRepository for MemoryPlanRepository {
    fn create(&self, plan: StoredPlan) -> Result<StoredPlan, RepositoryError> {
        let mut plans = self
            .store
            .write()
            .map_err(|_| RepositoryError::Unavailable)?;
        if plans.contains_key(&plan.id) {
            return Err(RepositoryError::AlreadyExists);
        }
        plans.insert(plan.id, plan.clone());
        Ok(plan)
    }
    fn update(
        &self,
        plan_id: PlanId,
        expected_revision: u64,
        profile: PlanProfile,
        accounts: Vec<Account>,
        updated_at: SystemTime,
    ) -> Result<StoredPlan, RepositoryError> {
        let mut plans = self
            .store
            .write()
            .map_err(|_| RepositoryError::Unavailable)?;
        let stored = plans.get_mut(&plan_id).ok_or(RepositoryError::NotFound)?;
        if stored.revision != expected_revision {
            return Err(RepositoryError::RevisionConflict {
                actual_revision: stored.revision,
            });
        }
        stored.revision = stored
            .revision
            .checked_add(1)
            .ok_or(RepositoryError::Unavailable)?;
        stored.profile = profile;
        stored.accounts = accounts;
        stored.updated_at = updated_at;
        Ok(stored.clone())
    }
    fn get(&self, plan_id: PlanId) -> Result<StoredPlan, RepositoryError> {
        self.store
            .read()
            .map_err(|_| RepositoryError::Unavailable)?
            .get(&plan_id)
            .cloned()
            .ok_or(RepositoryError::NotFound)
    }
    fn delete(&self, plan_id: PlanId) -> Result<(), RepositoryError> {
        self.store
            .write()
            .map_err(|_| RepositoryError::Unavailable)?
            .remove(&plan_id);
        Ok(())
    }
    fn count(&self) -> Result<usize, RepositoryError> {
        Ok(self
            .store
            .read()
            .map_err(|_| RepositoryError::Unavailable)?
            .len())
    }
    fn ready(&self) -> Result<(), RepositoryError> {
        drop(
            self.store
                .read()
                .map_err(|_| RepositoryError::Unavailable)?,
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::MemoryPlanRepository;
    use application::{PlanRepository, RepositoryError};
    use domain::{PlanId, PlanProfile, RetirementPlan};
    use std::{sync::Barrier, thread, time::SystemTime};
    use uuid::Uuid;

    fn plan(id: PlanId) -> RetirementPlan {
        RetirementPlan {
            id,
            revision: 1,
            profile: PlanProfile {
                current_age: 40,
                current_annual_income_cents: 10_000_000,
                projection_years: 20,
            },
            accounts: vec![],
            created_at: SystemTime::UNIX_EPOCH,
            updated_at: SystemTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn stale_concurrent_writer_cannot_mutate_the_winner() {
        let repository = MemoryPlanRepository::new();
        let id = PlanId::new(Uuid::new_v4());
        repository.create(plan(id)).expect("create");
        let barrier = std::sync::Arc::new(Barrier::new(3));
        let mut handles = Vec::new();
        for age in [41, 42] {
            let repository = repository.clone();
            let barrier = barrier.clone();
            handles.push(thread::spawn(move || {
                barrier.wait();
                repository.update(
                    id,
                    1,
                    PlanProfile {
                        current_age: age,
                        current_annual_income_cents: 10_000_000,
                        projection_years: 20,
                    },
                    vec![],
                    SystemTime::now(),
                )
            }));
        }
        barrier.wait();
        let results = handles
            .into_iter()
            .map(|handle| handle.join().expect("writer"))
            .collect::<Vec<_>>();
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter(|result| matches!(result, Err(RepositoryError::RevisionConflict { .. })))
                .count(),
            1
        );
        assert_eq!(repository.get(id).expect("stored").revision, 2);
    }

    #[test]
    fn poison_maps_to_unavailable() {
        let repository = MemoryPlanRepository::new();
        let store = repository.shared_store();
        let _ = thread::spawn(move || {
            let _guard = store.write().expect("lock");
            panic!("intentional poison");
        })
        .join();
        assert_eq!(repository.ready(), Err(RepositoryError::Unavailable));
    }
}
