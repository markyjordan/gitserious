use crate::CustomConfiguration;

/// Persists one global custom-configuration snapshot atomically.
pub trait GlobalConfigurationStore {
    /// Adapter-specific persistence failure.
    type Error;

    /// Loads the complete current snapshot.
    ///
    /// # Errors
    ///
    /// Returns the adapter's [`Self::Error`] when storage cannot be read or
    /// decoded safely.
    fn load(&self) -> Result<CustomConfiguration, Self::Error>;

    /// Atomically replaces `expected` with `replacement`.
    ///
    /// # Errors
    ///
    /// Returns the adapter's [`Self::Error`] when the snapshot changed after
    /// loading or replacement cannot be completed safely.
    fn compare_and_swap(
        &self,
        expected: &CustomConfiguration,
        replacement: &CustomConfiguration,
    ) -> Result<(), Self::Error>;
}
