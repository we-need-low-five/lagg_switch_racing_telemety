use std::ffi::c_void;
use std::marker::PhantomData;
use thiserror::Error;
use windows::core::PCWSTR;
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Memory::{
    MapViewOfFile, OpenFileMappingW, UnmapViewOfFile, FILE_MAP_READ,
};

#[derive(Error, Debug)]
pub enum SharedMemoryError {
    #[error("failed to open shared memory mapping: {0}")]
    OpenFailed(String),
    #[error("failed to map view of file")]
    MapFailed,
}

/// Raw byte mapping for game shared memory segments with a known size.
pub struct SharedMemoryMapping {
    handle: windows::Win32::Foundation::HANDLE,
    ptr: *const u8,
    size: usize,
}

unsafe impl Send for SharedMemoryMapping {}
unsafe impl Sync for SharedMemoryMapping {}

impl SharedMemoryMapping {
    pub fn open(name: &str, size: usize) -> Result<Self, SharedMemoryError> {
        let wide = to_wide(name);
        let handle = unsafe { OpenFileMappingW(FILE_MAP_READ.0, false, PCWSTR(wide.as_ptr())) }
            .map_err(|e| SharedMemoryError::OpenFailed(format!("{name}: {e}")))?;

        let map_size = if size == 0 { 0 } else { size };
        let ptr = unsafe { MapViewOfFile(handle, FILE_MAP_READ, 0, 0, map_size) };
        if ptr.Value.is_null() {
            unsafe {
                let _ = CloseHandle(handle);
            }
            return Err(SharedMemoryError::MapFailed);
        }

        Ok(Self {
            handle,
            ptr: ptr.Value as *const u8,
            size,
        })
    }

    pub fn is_open(name: &str, size: usize) -> bool {
        Self::open(name, size).is_ok()
    }

    pub fn read_pod<T: Copy>(&self) -> T {
        debug_assert!(std::mem::size_of::<T>() <= self.size);
        unsafe { std::ptr::read_unaligned(self.ptr as *const T) }
    }

    /// Copy a POD value out of the mapping at `byte_offset`. Use for large
    /// segmented mappings (e.g. LMU) where reading the whole layout every poll
    /// is wasteful and only a sub-struct is needed. When the mapping was opened
    /// with `size == 0` (map the entire section) the bound check is skipped.
    pub fn read_pod_at<T: Copy>(&self, byte_offset: usize) -> T {
        debug_assert!(
            self.size == 0 || byte_offset + std::mem::size_of::<T>() <= self.size,
            "read_pod_at past end of shared memory mapping"
        );
        unsafe { std::ptr::read_unaligned(self.ptr.add(byte_offset) as *const T) }
    }

    pub fn read_utf16_string_at(&self, offset: usize, max_chars: usize) -> String {
        let mut units = Vec::with_capacity(max_chars);
        for index in 0..max_chars {
            let byte_offset = offset + index * 2;
            if byte_offset + 2 > self.size {
                break;
            }
            let unit = unsafe {
                std::ptr::read_unaligned(self.ptr.add(byte_offset) as *const u16)
            };
            if unit == 0 {
                break;
            }
            units.push(unit);
        }
        String::from_utf16_lossy(&units)
    }
}

impl Drop for SharedMemoryMapping {
    fn drop(&mut self) {
        unsafe {
            let _ = UnmapViewOfFile(windows::Win32::System::Memory::MEMORY_MAPPED_VIEW_ADDRESS {
                Value: self.ptr as *mut c_void,
            });
            let _ = CloseHandle(self.handle);
        }
    }
}

pub struct SharedMemoryView<T> {
    mapping: SharedMemoryMapping,
    _marker: PhantomData<T>,
}

unsafe impl<T> Send for SharedMemoryView<T> {}
unsafe impl<T> Sync for SharedMemoryView<T> {}

impl<T: Copy> SharedMemoryView<T> {
    pub fn open(name: &str, size: usize) -> Result<Self, SharedMemoryError> {
        let mapping = SharedMemoryMapping::open(name, size)?;
        Ok(Self {
            mapping,
            _marker: PhantomData,
        })
    }

    pub fn read(&self) -> T {
        self.mapping.read_pod()
    }
}

pub fn to_wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn utf16_to_string(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}
