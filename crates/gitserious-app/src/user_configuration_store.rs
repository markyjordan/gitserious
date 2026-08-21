use crate::UserConfiguration;

/// Persists one global user-configuration snapshot atomically.
pub trait UserConfigurationStore {
    /// Adapter-specific persistence failure.
    type Error;

    /// Loads the complete current snapshot.
    ///
    /// # Errors
    ///
    /// Returns the adapter's [`Self::Error`] when storage cannot be read or
    /// decoded safely.
    fn load(&self) -> Result<UserConfiguration, Self::Error>;

    /// Atomically replaces `expected` with `replacement`.
    ///
    /// # Errors
    ///
    /// Returns the adapter's [`Self::Error`] when the snapshot changed after
    /// loading or replacement cannot be completed safely.
    fn compare_and_swap(
        &self,
        expected: &UserConfiguration,
        replacement: &UserConfiguration,
    ) -> Result<(), Self::Error>;
}
