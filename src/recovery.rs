use crate::error::Result;
use crate::store::Store;

pub fn recover(store: &Store) -> Result<()> {
    let _ = store.fail_stale_running_jobs()?;
    Ok(())
}
