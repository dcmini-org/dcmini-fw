/// Errors that can occur during bus operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum BusError<E: core::fmt::Debug> {
    /// The bus factory failed to create the bus.
    FactoryError(E),
    /// Bus is currently in use by `n` handles and cannot be released.
    InUse(usize),
    /// Bus manager is in an unrecoverable state.
    Poisoned,
}
