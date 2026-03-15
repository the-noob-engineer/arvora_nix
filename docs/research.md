# Building a Windows Compatibility Layer in Rust (from scratch)

This document explains how a Windows compatibility layer (like Wine) works under the hood,
and how you could implement the basic pieces in Rust.

---

## Overview

A Windows `.exe` is a **Portable Executable (PE)** file. It expects Windows services (`kernel32.dll`, `user32.dll`, etc.) and calls Windows APIs.
A compatibility layer must:

1. **Load the PE file** into memory.
2. **Resolve imports** and redirect them to your own implementations (shims).
3. **Translate Windows API calls** into Linux syscalls or libraries.
4. **Run the program natively** on your CPU (no emulation).

---

## Steps to Intercept and Run `.exe` Files

### 1. Parse the PE
Use a crate like [`goblin`](https://docs.rs/goblin) or `pelite` to parse the `.exe`.

```rust
use goblin::pe::PE;
use std::{fs::File, io::Read};

fn main() -> anyhow::Result<()> {
    let mut buf = Vec::new();
    File::open("program.exe")?.read_to_end(&mut buf)?;
    let pe = PE::parse(&buf)?;

    if let Some(imports) = &pe.imports {
        for imp in imports {
            println!("DLL: {}", imp.name);
            for thunk in &imp.imports {
                if let Some(name) = &thunk.name {
                    println!("  {}!{}", imp.name, name);
                }
            }
        }
    }
    Ok(())
}
```

2. ## Map Sections into Memory

```rust
use goblin::pe::{PE, section_table::SectionTable};
use nix::sys::mman::{mmap, MapFlags, ProtFlags};
use std::{ptr};

unsafe fn map_image(buf: &[u8], pe: &PE) -> anyhow::Result<*mut u8> {
    let alloc_size = pe.image.unwrap().SizeOfImage as usize;

    let base = mmap(
        ptr::null_mut(),
        alloc_size,
        ProtFlags::PROT_READ | ProtFlags::PROT_WRITE | ProtFlags::PROT_EXEC,
        MapFlags::MAP_PRIVATE | MapFlags::MAP_ANONYMOUS,
        -1,
        0,
    )? as *mut u8;

    // Copy headers + sections
    ptr::copy_nonoverlapping(buf.as_ptr(), base, pe.image.unwrap().SizeOfHeaders as usize);

    for s in &pe.sections {
        map_section(base, buf, s)?;
    }

    Ok(base)
}

unsafe fn map_section(base: *mut u8, buf: &[u8], s: &SectionTable) -> anyhow::Result<()> {
    let virt = s.virtual_address as usize;
    let raw_ptr = buf.as_ptr().add(s.pointer_to_raw_data as usize);
    let dst = base.add(virt);
    let size = s.size_of_raw_data as usize;
    ptr::copy_nonoverlapping(raw_ptr, dst, size);
    Ok(())
}
```

3. ## Stub and Patch Imports (Intercept!)
Example: `kernel32!CreateFileA` → Linux `open(2)`

```rust
#[repr(C)]
pub struct WinHandle(pub isize);

#[no_mangle]
pub extern "system" fn CreateFileA(
    lpFileName: *const i8,
    _dwDesiredAccess: u32,
    _dwShareMode: u32,
    _lpSecurityAttributes: *mut core::ffi::c_void,
    _dwCreationDisposition: u32,
    _dwFlagsAndAttributes: u32,
    _hTemplateFile: WinHandle,
) -> WinHandle {
    use std::ffi::CStr;
    let cstr = unsafe { CStr::from_ptr(lpFileName) };
    let path = cstr.to_string_lossy();

    let fd = nix::fcntl::open(
        path.as_ref(),
        nix::fcntl::OFlag::O_RDWR | nix::fcntl::O_CREAT,
        nix::sys::stat::Mode::from_bits_truncate(0o644),
    ).unwrap_or(-1);

    WinHandle(fd as isize)
}
```

Patch the Import Address Table (IAT):
```rust
unsafe fn patch_import(
    image_base: *mut u8,
    dll: &str,
    func: &str,
    replacement: *const (),
    pe: &goblin::pe::PE,
) {
    let imports = pe.imports.as_ref().unwrap();
    for imp in imports {
        if imp.name.eq_ignore_ascii_case(dll) {
            for thunk in &imp.imports {
                if let Some(name) = &thunk.name {
                    if name == func {
                        let iat_rva = thunk.iat as usize;
                        let iat_ptr = image_base.add(iat_rva) as *mut usize;
                        *iat_ptr = replacement as usize;
                    }
                }
            }
        }
    }
}
```

4. ## Jump to the Entry Point

```rust
type Entry = extern "system" fn() -> i32;

unsafe fn call_entry(image_base: *mut u8, pe: &PE) -> i32 {
    let entry_rva = pe.entry as usize;
    let entry: Entry = std::mem::transmute(image_base.add(entry_rva));
    entry()
}
```

## Challenges

- Relocations: Handle base relocation table if image not loaded at preferred base.
- TLS & SEH: Thread Local Storage and Structured Exception Handling must be emulated.
- Windows Calling Conventions: Windows x64 ABI differs from System V ABI.
- PEB/TEB Structures: Windows expects process/thread environment blocks.

- Subsystems:
  - File system, registry → `kernel32`, `advapi32`.
  - Windowing/input → `user32`, `gdi32` → X11/Wayland.
  - Networking → sockets.
  - Graphics → DirectX → Vulkan (DXVK or your own translator).

## Practical Plan
1. Run a minimal PE (no imports).
2. Implement ExitProcess, GetLastError, CreateFileA.
3. Add IAT patching + logging for missing functions.
4. Add threading & sync (map to pthread/futex).
5. Add console I/O and file I/O.
6. Only then tackle GUI and graphics.
