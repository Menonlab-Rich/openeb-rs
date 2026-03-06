use crate::hal::facilities::{FacilityHandle, FacilityType};

pub trait Device {
    /// Retrieves a specific facility handle if it exists.
    fn get_facility(&self, facility_type: FacilityType) -> Option<FacilityHandle>;

    /// Returns an owned list of available facilities, derived from the internal map keys.
    fn get_facilities(&self) -> Vec<FacilityType>;

    /// Registers a new facility. Requires exclusive mutable access.
    fn register_facility(
        &mut self,
        facility_type: FacilityType,
        facility_handle: FacilityHandle,
    ) -> Option<FacilityHandle>;
}
