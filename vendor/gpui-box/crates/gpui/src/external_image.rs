use crate::{DevicePixels, Size};
use std::{
    any::Any,
    fmt,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

static NEXT_EXTERNAL_IMAGE_ID: AtomicUsize = AtomicUsize::new(0);

/// Identifies an external image for the lifetime of this process.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct ExternalImageId(pub usize);

/// A backend-neutral image whose payload is interpreted by the active renderer.
#[derive(Clone)]
pub struct ExternalImageHandle {
    id: ExternalImageId,
    size: Size<DevicePixels>,
    #[cfg(not(target_family = "wasm"))]
    payload: Arc<dyn Any + Send + Sync>,
    #[cfg(target_family = "wasm")]
    payload: Arc<dyn Any>,
}

impl ExternalImageHandle {
    /// Creates an external image with its intrinsic physical size.
    #[cfg(not(target_family = "wasm"))]
    pub fn new<T: Any + Send + Sync>(size: Size<DevicePixels>, payload: T) -> Self {
        Self {
            id: ExternalImageId(NEXT_EXTERNAL_IMAGE_ID.fetch_add(1, Ordering::Relaxed)),
            size,
            payload: Arc::new(payload),
        }
    }

    /// Creates a main-thread WebGPU image with its intrinsic physical size.
    #[cfg(target_family = "wasm")]
    pub fn new<T: Any>(size: Size<DevicePixels>, payload: T) -> Self {
        Self {
            id: ExternalImageId(NEXT_EXTERNAL_IMAGE_ID.fetch_add(1, Ordering::Relaxed)),
            size,
            payload: Arc::new(payload),
        }
    }

    /// Returns the process-local identifier used for renderer batching.
    pub fn id(&self) -> ExternalImageId {
        self.id
    }

    /// Returns the intrinsic physical size.
    pub fn size(&self) -> Size<DevicePixels> {
        self.size
    }

    /// Returns the renderer payload when it has the requested concrete type.
    pub fn downcast_ref<T: Any>(&self) -> Option<&T> {
        self.payload.downcast_ref()
    }
}

impl fmt::Debug for ExternalImageHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExternalImageHandle")
            .field("id", &self.id)
            .field("size", &self.size)
            .finish_non_exhaustive()
    }
}
