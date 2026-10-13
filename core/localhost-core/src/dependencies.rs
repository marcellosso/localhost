//! The Dependency graph of a Project's Services. Dependencies are validated
//! when edited, so the graph of a stored Project never has a cycle.

use std::collections::HashSet;

use crate::{Id, Key, Project, Service};

impl Project {
    /// The Services that depend directly on `service_id`.
    pub fn dependents(&self, service_id: Id) -> Vec<&Service> {
        self.services.iter().filter(|service| service.dependencies.contains(&service_id)).collect()
    }

    /// The cycle that making `service_id` depend on `dependency_id` would
    /// close, as Keys from `service_id` back to itself (e.g. `api → worker →
    /// api`), or `None` if the Dependency is allowed.
    pub fn dependency_cycle(&self, service_id: Id, dependency_id: Id) -> Option<Vec<Key>> {
        let service = self.service(service_id)?;
        if service_id == dependency_id {
            return Some(vec![service.key.clone(), service.key.clone()]);
        }
        // Depth-first search for a path dependency_id → … → service_id.
        fn path(project: &Project, id: Id, target: Id, visited: &mut HashSet<Id>) -> Option<Vec<Key>> {
            if !visited.insert(id) {
                return None;
            }
            let current = project.service(id)?;
            if id == target {
                return Some(vec![current.key.clone()]);
            }
            for &next in &current.dependencies {
                if let Some(rest) = path(project, next, target, visited) {
                    let mut keys = vec![current.key.clone()];
                    keys.extend(rest);
                    return Some(keys);
                }
            }
            None
        }
        let rest = path(self, dependency_id, service_id, &mut HashSet::new())?;
        let mut cycle = vec![service.key.clone()];
        cycle.extend(rest);
        Some(cycle)
    }

    /// `service_ids` ordered so that every Service comes after its
    /// Dependencies among them; otherwise in the Project's order.
    pub fn dependency_order(&self, service_ids: &[Id]) -> Vec<Id> {
        let included: HashSet<Id> = service_ids.iter().copied().collect();
        let mut ordered = Vec::new();
        let mut placed = HashSet::new();
        fn place(project: &Project, id: Id, included: &HashSet<Id>, placed: &mut HashSet<Id>, ordered: &mut Vec<Id>) {
            if !included.contains(&id) || placed.contains(&id) {
                return;
            }
            let Some(service) = project.service(id) else { return };
            placed.insert(id);
            for &dependency in &service.dependencies {
                place(project, dependency, included, placed, ordered);
            }
            ordered.push(id);
        }
        for service in &self.services {
            place(self, service.id, &included, &mut placed, &mut ordered);
        }
        ordered
    }
}
