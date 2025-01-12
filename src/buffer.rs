use std::{
    ffi::CStr,
    fs::File,
    io,
    os::unix::io::{FromRawFd, RawFd},
    time::SystemTime,
    time::UNIX_EPOCH,
};

#[cfg(target_os = "linux")]
use nix::sys::memfd;
use nix::{
    errno::Errno,
    fcntl,
    sys::{mman, stat},
    unistd,
};

use memmap::MmapMut;

use wayland_client::{
    protocol::{wl_buffer, wl_shm, wl_shm_pool},
    Dispatch, QueueHandle,
};

use crate::color::Color;
use crate::widgets::Geometry;

fn create_shm_fd() -> io::Result<RawFd> {
    // Only try memfd on linux
    #[cfg(target_os = "linux")]
    loop {
        match memfd::memfd_create(
            CStr::from_bytes_with_nul(b"wldash\0").unwrap(),
            memfd::MemFdCreateFlag::MFD_CLOEXEC,
        ) {
            Ok(fd) => return Ok(fd),
            Err(Errno::EINTR) => continue,
            Err(Errno::ENOSYS) => break,
            Err(errno) => return Err(io::Error::from(errno)),
        }
    }

    // Fallback to using shm_open
    let sys_time = SystemTime::now();
    let mut mem_file_handle = format!(
        "/wldash-{}",
        sys_time.duration_since(UNIX_EPOCH).unwrap().subsec_nanos()
    );
    loop {
        match mman::shm_open(
            mem_file_handle.as_str(),
            fcntl::OFlag::O_CREAT
                | fcntl::OFlag::O_EXCL
                | fcntl::OFlag::O_RDWR
                | fcntl::OFlag::O_CLOEXEC,
            stat::Mode::S_IRUSR | stat::Mode::S_IWUSR,
        ) {
            Ok(fd) => match mman::shm_unlink(mem_file_handle.as_str()) {
                Ok(_) => return Ok(fd),
                Err(errno) => match unistd::close(fd) {
                    Ok(_) => return Err(io::Error::from(errno)),
                    Err(errno) => return Err(io::Error::from(errno)),
                },
            },
            Err(Errno::EEXIST) => {
                // If a file with that handle exists then change the handle
                mem_file_handle = format!(
                    "/wldash-{}",
                    sys_time.duration_since(UNIX_EPOCH).unwrap().subsec_nanos()
                );
                continue;
            }
            Err(Errno::EINTR) => continue,
            Err(errno) => return Err(io::Error::from(errno)),
        }
    }
}

pub struct ShmBuffer {
    pub file: File,
    pub mmap: MmapMut,
    pub buffer: wl_buffer::WlBuffer,
    pub refcnt: u32,
    pub id: u32,
}

impl ShmBuffer {
    fn new<D>(
        shm: &wl_shm::WlShm,
        qh: &QueueHandle<D>,
        width: i32,
        height: i32,
        stride: i32,
        format: wl_shm::Format,
        id: u32,
    ) -> io::Result<ShmBuffer>
    where
        D: Dispatch<wl_shm_pool::WlShmPool, ()> + Dispatch<wl_buffer::WlBuffer, ()> + 'static,
    {
        let mem_fd = create_shm_fd()?;
        let file = unsafe { File::from_raw_fd(mem_fd) };
        file.set_len((height * stride) as u64)?;
        let pool = shm.create_pool(mem_fd, height * stride, &qh, ());
        let mmap = unsafe { MmapMut::map_mut(&file).unwrap() };
        let buffer = pool.create_buffer(0, width, height, stride, format, &qh, ());

        Ok(ShmBuffer {
            refcnt: 0,
            file,
            mmap,
            buffer,
            id,
        })
    }

    pub fn acquire(&mut self) {
        self.refcnt += 1;
    }

    pub fn release(&mut self) {
        self.refcnt -= 1;
    }
}

pub struct BufferManager {
    pub buffers: Vec<ShmBuffer>,
    pub next_id: u32,
}

impl BufferManager {
    pub fn new() -> BufferManager {
        BufferManager {
            buffers: Vec::new(),
            next_id: 0,
        }
    }

    pub fn clear_buffers(&mut self) {
        for buf in self.buffers.iter_mut() {
            buf.buffer.destroy()
        }
        self.buffers.clear()
    }

    pub fn next_buffer(&mut self) -> Option<&mut ShmBuffer> {
        for buf in self.buffers.iter_mut() {
            if buf.refcnt == 0 {
                return Some(buf);
            }
        }
        None
    }

    pub fn add_buffer<D>(
        &mut self,
        wl_shm: &wl_shm::WlShm,
        dimensions: (i32, i32),
        qh: &QueueHandle<D>,
    ) where
        D: Dispatch<wl_shm_pool::WlShmPool, ()> + Dispatch<wl_buffer::WlBuffer, ()> + 'static,
    {
        let mut buf = ShmBuffer::new(
            wl_shm,
            qh,
            dimensions.0,
            dimensions.1,
            dimensions.0 * 4,
            wl_shm::Format::Argb8888,
            self.next_id,
        )
        .expect("unable to add buffer");
        self.next_id += 1;
        BufferView::new(&mut buf.mmap, (dimensions.0 as u32, dimensions.1 as u32));
        self.buffers.push(buf);
    }
}

pub struct BufferView<'a> {
    view: &'a mut [u32],
    dimensions: (u32, u32),
    subdimensions: Option<(u32, u32, u32, u32)>,
}

impl<'a> BufferView<'a> {
    pub fn new(buf: &'a mut MmapMut, dimensions: (u32, u32)) -> BufferView {
        let view = unsafe {
            std::slice::from_raw_parts_mut(
                buf.as_mut_ptr() as *mut u32,
                (dimensions.0 * dimensions.1) as usize,
            )
        };
        BufferView {
            view,
            dimensions,
            subdimensions: None,
        }
    }

    #[inline]
    pub fn get_bounds(&self) -> (u32, u32, u32, u32) {
        if let Some(subdim) = self.subdimensions {
            subdim
        } else {
            (0, 0, self.dimensions.0, self.dimensions.1)
        }
    }

    pub fn subdimensions(&mut self, subdimensions: (u32, u32, u32, u32)) -> BufferView {
        let bounds = self.get_bounds();
        if cfg!(debug_assertions)
            && (subdimensions.0 + subdimensions.2 > bounds.2
                || subdimensions.1 + subdimensions.3 > bounds.3)
        {
            panic!(
                "cannot create subdimensions larger than buffer: {:?} > {:?}",
                subdimensions, bounds
            );
        }

        BufferView {
            view: self.view,
            dimensions: self.dimensions,
            subdimensions: Some((
                subdimensions.0 + bounds.0,
                subdimensions.1 + bounds.1,
                subdimensions.2,
                subdimensions.3,
            )),
        }
    }

    pub fn subgeometry(&mut self, geo: Geometry) -> BufferView {
        self.subdimensions((geo.x, geo.y, geo.width, geo.height))
    }

    pub fn offset(&mut self, offset: (u32, u32)) -> BufferView {
        let bounds = self.get_bounds();
        if cfg!(debug_assertions) && (offset.0 > bounds.2 || offset.1 > bounds.3) {
            panic!(
                "cannot create offset outside buffer: {:?} > {:?}",
                offset, bounds
            );
        }

        BufferView {
            view: self.view,
            dimensions: self.dimensions,
            subdimensions: Some((
                offset.0 + bounds.0,
                offset.1 + bounds.1,
                bounds.2 - offset.0,
                bounds.3 - offset.1,
            )),
        }
    }

    pub fn memset(&mut self, c: Color) {
        if let Some(subdim) = self.subdimensions {
            for y in subdim.1..(subdim.1 + subdim.3) {
                let start = (subdim.0 + y * self.dimensions.0) as usize;
                let end = start + subdim.2 as usize;
                self.view[start..end].fill(c.0);
            }
        } else {
            self.view.fill(c.0);
        }
    }

    pub fn clear(&mut self) {
        if let Some(subdim) = self.subdimensions {
            for y in subdim.1..(subdim.1 + subdim.3) {
                let start = (subdim.0 + y * self.dimensions.0) as usize;
                let end = start + subdim.2 as usize;
                self.view[start..end].fill(0x0);
            }
        } else {
            self.view.fill(0x0);
        }
    }

    #[inline]
    pub fn put_raw(&mut self, pos: (u32, u32), c: Color) {
        self.view[(pos.0 + pos.1 * self.dimensions.0) as usize] = c.0
    }

    #[inline]
    pub fn put_line_raw(&mut self, pos: (u32, u32), len: u32, c: Color) {
        let start = (pos.0 + (pos.1 * self.dimensions.0)) as usize;
        let len = len as usize;
        self.view[start..start + len].fill(c.0);
    }
}
