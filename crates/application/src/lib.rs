//! Application ports and retirement-plan use cases.

use std::{error::Error, fmt, time::SystemTime};

use domain::{
    Account, PlanId, PlanProfile, Projection, ProjectionError, RetirementPlan, ValidationErrors,
};

pub type StoredPlan = RetirementPlan;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RepositoryError {
    AlreadyExists,
    NotFound,
    RevisionConflict { actual_revision: u64 },
    Unavailable,
}

impl fmt::Display for RepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlreadyExists => formatter.write_str("plan already exists"),
            Self::NotFound => formatter.write_str("plan not found"),
            Self::RevisionConflict { .. } => formatter.write_str("plan revision conflict"),
            Self::Unavailable => formatter.write_str("repository unavailable"),
        }
    }
}

impl Error for RepositoryError {}

pub trait PlanRepository: Clone + Send + Sync + 'static {
    /// Atomically inserts a new plan.
    ///
    /// # Errors
    /// Returns `AlreadyExists` or `Unavailable` when insertion cannot complete.
    fn create(&self, plan: StoredPlan) -> Result<StoredPlan, RepositoryError>;
    /// Atomically checks the revision and replaces editable fields.
    ///
    /// # Errors
    /// Returns `NotFound`, `RevisionConflict`, or `Unavailable` as applicable.
    fn update(
        &self,
        plan_id: PlanId,
        expected_revision: u64,
        profile: PlanProfile,
        accounts: Vec<Account>,
        updated_at: SystemTime,
    ) -> Result<StoredPlan, RepositoryError>;
    /// Returns an owned snapshot.
    ///
    /// # Errors
    /// Returns `NotFound` or `Unavailable`.
    fn get(&self, plan_id: PlanId) -> Result<StoredPlan, RepositoryError>;
    /// Idempotently deletes a plan.
    ///
    /// # Errors
    /// Returns `Unavailable` when storage cannot be accessed.
    fn delete(&self, plan_id: PlanId) -> Result<(), RepositoryError>;
    /// Counts saved plans.
    ///
    /// # Errors
    /// Returns `Unavailable` when storage cannot be accessed.
    fn count(&self) -> Result<usize, RepositoryError>;
    /// Probes repository readiness.
    ///
    /// # Errors
    /// Returns `Unavailable` when storage cannot be accessed.
    fn ready(&self) -> Result<(), RepositoryError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreatePlan {
    pub id: PlanId,
    pub profile: PlanProfile,
    pub accounts: Vec<Account>,
    pub now: SystemTime,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpdatePlan {
    pub id: PlanId,
    pub expected_revision: u64,
    pub profile: PlanProfile,
    pub accounts: Vec<Account>,
    pub now: SystemTime,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GetPlan {
    pub id: PlanId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeletePlan {
    pub id: PlanId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectPlan {
    Stored {
        id: PlanId,
    },
    Stateless {
        profile: PlanProfile,
        accounts: Vec<Account>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectedPlan {
    pub stored_plan: Option<StoredPlan>,
    pub projection: Projection,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ApplicationError {
    Validation(ValidationErrors),
    Projection(ProjectionError),
    Repository(RepositoryError),
}

impl fmt::Display for ApplicationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Validation(error) => error.fmt(formatter),
            Self::Projection(error) => error.fmt(formatter),
            Self::Repository(error) => error.fmt(formatter),
        }
    }
}
impl Error for ApplicationError {}
impl From<ValidationErrors> for ApplicationError {
    fn from(value: ValidationErrors) -> Self {
        Self::Validation(value)
    }
}
impl From<ProjectionError> for ApplicationError {
    fn from(value: ProjectionError) -> Self {
        Self::Projection(value)
    }
}
impl From<RepositoryError> for ApplicationError {
    fn from(value: RepositoryError) -> Self {
        Self::Repository(value)
    }
}

#[derive(Clone, Debug)]
pub struct Application<R> {
    repository: R,
}

impl<R: PlanRepository> Application<R> {
    #[must_use]
    pub const fn new(repository: R) -> Self {
        Self { repository }
    }
    #[must_use]
    pub const fn repository(&self) -> &R {
        &self.repository
    }

    /// Validates and creates a plan.
    ///
    /// # Errors
    /// Returns validation or repository failures without partial mutation.
    pub fn create(&self, command: CreatePlan) -> Result<StoredPlan, ApplicationError> {
        let plan = RetirementPlan {
            id: command.id,
            revision: 1,
            profile: command.profile,
            accounts: command.accounts,
            created_at: command.now,
            updated_at: command.now,
        };
        plan.validate_for_save()?;
        Ok(self.repository.create(plan)?)
    }

    /// Validates and optimistically updates a plan.
    ///
    /// # Errors
    /// Returns validation, missing-plan, stale-revision, or availability failures.
    pub fn update(&self, command: UpdatePlan) -> Result<StoredPlan, ApplicationError> {
        domain::validate_for_save(&command.profile, &command.accounts)?;
        Ok(self.repository.update(
            command.id,
            command.expected_revision,
            command.profile,
            command.accounts,
            command.now,
        )?)
    }

    /// Fetches an owned snapshot.
    ///
    /// # Errors
    /// Returns missing-plan or availability failures.
    pub fn get(&self, query: GetPlan) -> Result<StoredPlan, ApplicationError> {
        Ok(self.repository.get(query.id)?)
    }
    /// Idempotently deletes a plan.
    ///
    /// # Errors
    /// Returns an availability failure if storage cannot be accessed.
    pub fn delete(&self, command: DeletePlan) -> Result<(), ApplicationError> {
        Ok(self.repository.delete(command.id)?)
    }

    /// Projects a stored snapshot or stateless input through the same domain function.
    ///
    /// # Errors
    /// Returns repository, validation, or calculation failures without partial output.
    pub fn project(&self, query: ProjectPlan) -> Result<ProjectedPlan, ApplicationError> {
        match query {
            ProjectPlan::Stored { id } => {
                let snapshot = self.repository.get(id)?;
                let projection = domain::project(&snapshot.profile, &snapshot.accounts)?;
                Ok(ProjectedPlan {
                    stored_plan: Some(snapshot),
                    projection,
                })
            }
            ProjectPlan::Stateless { profile, accounts } => Ok(ProjectedPlan {
                projection: domain::project(&profile, &accounts)?,
                stored_plan: None,
            }),
        }
    }
}
