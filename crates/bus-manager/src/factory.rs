/// Abstracts bus creation and destruction.
///
/// Implementors define how to create a bus from resources and how to
/// recover those resources when the bus is torn down.
pub trait BusFactory {
    /// The bus type that will be shared among handles.
    type Bus;
    /// Resources needed to create the bus (e.g., peripheral handles, pins).
    type Resources;
    /// Opaque token that can reconstruct [`Resources`](Self::Resources) after the bus is dropped.
    type Destructor;
    /// Error type for bus creation failures.
    type Error: core::fmt::Debug;

    /// Create a bus from the given resources.
    ///
    /// On success, returns the bus instance and a destructor token.
    /// On failure, returns the error **and** the original resources so they are not lost.
    fn create(
        resources: Self::Resources,
    ) -> Result<(Self::Bus, Self::Destructor), (Self::Error, Self::Resources)>;

    /// Recover the original resources from a destructor token.
    fn recover(destructor: Self::Destructor) -> Self::Resources;
}
