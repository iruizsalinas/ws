//! Bound work per connection even for transports that are always ready.
//! Yield after bytes of progress, rather than paying a scheduler hop per read
//! or small write. The budget persists across messages and queue batches.
#[derive(Default)]
pub(super) struct Budget(usize);

pub(super) const CHUNK: usize = 64 * 1024;

impl Budget {
    pub async fn progress(&mut self, bytes: usize) {
        self.0 = self.0.saturating_add(bytes);
        if self.0 >= CHUNK {
            self.0 = 0;
            tokio::task::yield_now().await;
        }
    }
}
