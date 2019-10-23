# `μAMP`

> A (micro) framework for building bare-metal AMP (Asymmetric Multi-Processing) applications

## Features

- Lets you write single-source multi-core applications

- Zero cost shared memory abstraction for Inter-Processor Communication.

- Use the built-in conditional compilation feature (`#[cfg(core = "0")]` /
  `cfg!(core = "1")`) to partition your application between the cores.

## Planned features

- Multi-target support.

The framework currently uses the *same* compilation target for all the cores.
This limits the supported systems to homogeneous multi-core devices (all cores
use the exact same instruction set) and heterogeneous multi-core devices where
the cores share a lowest common denominator instruction set (for example, both
cores in a Cortex-M4 + Cortex-M0 device can run programs compiled for
`thumbv6m-none-eabi`).

We need to design (and test) a command line flag to specify, or override, a
compilation target for each core.

## Known limitations

- The framework currently only supports the ARM architecture.

To lift this limitation we need to implement the functionality of the `strip`
command in Rust. Namely this invocation needs to be ported to Rust: `strip -R
'*' -R '!.shared' --strip-unneeded `. If you are aware of any crate that can do
that please let me know in the issue tracker!

## Known issues

- Sending / sharing a function pointer or trait object between the cores is
  potentially unsound, or at least a Very Bad idea but this is not *fully*
  rejected in the case of function pointers.

The framework tries to prevent this operation at compile time. All trait objects
are currently rejected, but only function pointers with 0 to 12 arguments are
rejected. To reject *all* function pointers we would need the Variadic Generics
(VG) language feature.

## Example

Here's a program that runs on a homogeneous dual-core device (2x Cortex-R5
cores).

``` rust, ignore
#![no_main]
#![no_std]

use core::sync::atomic::{AtomicU8, Ordering};

use arm_dcc::dprintln;
use microamp::shared;
use panic_dcc as _; // panic handler
use zup_rt::entry;

// non-atomic variable
#[shared] // <- means: same memory location on all the cores
static mut SHARED: u64 = 0;

// used to synchronize access to `SHARED`
#[shared]
static SEMAPHORE: AtomicU8 = AtomicU8::new(CORE0);

// possible values for SEMAPHORE
const CORE0: u8 = 0;
const CORE1: u8 = 1;
const LOCKED: u8 = 2;

#[entry]
fn main() -> ! {
    let (our_turn, next_core) = if cfg!(core = "0") {
        (CORE0, CORE1)
    } else {
        (CORE1, CORE0)
    };

    dprintln!("START");

    let mut done = false;
    while !done {
        // try to acquire the lock
        while SEMAPHORE
            .compare_exchange(our_turn, LOCKED, Ordering::AcqRel, Ordering::Relaxed)
            .is_err()
        {
            // busy wait if the lock is held by the other core
        }

        // we acquired the lock; now we have exclusive access to `SHARED`
        unsafe {
            if SHARED >= 10 {
                // stop at some arbitrary point
                done = true;
            } else {
                dprintln!("{}", SHARED);

                SHARED += 1;
            }
        }

        // release the lock & unblock the other core
        SEMAPHORE.store(next_core, Ordering::Release);
    }

    dprintln!("DONE");

    loop {}
}
```

In this example we have two static variables in shared memory and visible to
both cores (\*). One of the variables, `SEMAPHORE`, is used to synchronize
access to the non-atomic `SHARED` variable. Both cores will execute the `main`
function at boot but they will execute slightly different code paths due to the
use of the `cfg!` macro.

To build the application we use the following command:

``` console
$ cargo microamp --bin app --release
   Compiling zup-rtfm v0.1.0 (/tmp/firmware)
    Finished dev [unoptimized + debuginfo] target(s) in 0.32s
   Compiling zup-rtfm v0.1.0 (/tmp/firmware)
    Finished dev [unoptimized + debuginfo] target(s) in 0.12s
   Compiling zup-rtfm v0.1.0 (/tmp/firmware)
    Finished dev [unoptimized + debuginfo] target(s) in 0.12s
```

By default the command produces two images, one for each core.

``` console
$ # image for first core
$ size -Ax target/armv7r-none-eabi/release/examples/app-0
target/armv7r-none-eabi/release/examples/app-0  :
section             size         addr
.text              0x360          0x0
.local               0x0      0x20000
.bss                 0x0   0xfffc0000
.data                0x0   0xfffc0000
.rodata             0x40   0xfffc0000
.shared             0x10   0xfffe0000

$ # image for second core
$ size -Ax target/armv7r-none-eabi/release/examples/app-1
target/armv7r-none-eabi/release/examples/app-1  :
section             size         addr
.text              0x360          0x0
.local               0x0      0x20000
.bss                 0x0   0xfffd0000
.data                0x0   0xfffd0000
.rodata             0x40   0xfffd0000
.shared             0x10   0xfffe0000
```

If we run the image on core #0 we'll see:

``` console
$ # on another terminal: load and run the program
$ CORE=0 xsdb -interactive debug.tcl amp-shared-0

$ # output of core #0
$ tail -f dcc0.log
START
0
```

That the program halts because it's waiting for the other core. Now, we run the
other image on core #1.

``` console
$ # on another terminal: load and run the program
$ CORE=1 xsdb -interactive debug.tcl amp-shared-1

$ # output of core #1
$ tail -f dcc1.log
START
1
3
5
7
9
DONE
```

And we'll get new output from core #0.

``` console
$ # output of core #0
$ tail -f dcc0.log
START
0
2
4
6
8
DONE
```

(\*) **IMPORTANT** all static variables *not* marked as `#[shared]` will be
*instantiated* for each core and very likely have different addresses (even if
they have the same symbol name) due to compiler optimizations and linker script
differences. For example, a non-`#[shared]` variable `static mut X: u32` may
have address `0xffe20000` in one image and address `0xffeb0000` in the other
image.

## Requirements

The user, or a crate, must provide one linker script *per core*. The
`cargo-microamp` tool will use these linker scripts to link the program for each
core and expects them to be named `core0.x`, `core1.x`, etc.

`cargo-microamp` will pass a file named `microamp-data.o` to the linker when
linking each image. This object file contains all the `#[shared]` variables
in a section named `.shared`. These variables must be placed in an output
section named `.shared`. This section must be located at the *same* address on
all images. For example:

``` console
$ cat core0.x
SECTIONS
{
  /* .. */

  .shared : ALIGN(4)
  {
    KEEP(microamp-data.o(.shared));
    . = ALIGN(4);
  } > OCM0

  /* .. */
}
```

``` console
$ cat core1.x
SECTIONS
{
  /* .. */

  /* NOTE(NOLOAD) core 0 will initialize this shared section  */
  .shared (NOLOAD) : ALIGN(4)
  {
    KEEP(microamp-data.o(.shared));
    . = ALIGN(4);
  } > OCM0

  /* .. */
}
```

Furthermore care must be taken to *not* initialize this `.shared` link section
*more than once*. In the above example, the shared variables are initialized
when the *first* image is loaded into memory.

## License

All source code (including code snippets) is licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  [https://www.apache.org/licenses/LICENSE-2.0][L1])

- MIT license ([LICENSE-MIT](LICENSE-MIT) or
  [https://opensource.org/licenses/MIT][L2])

[L1]: https://www.apache.org/licenses/LICENSE-2.0
[L2]: https://opensource.org/licenses/MIT

at your option.

The written prose contained within the book is licensed under the terms of the
Creative Commons CC-BY-SA v4.0 license ([LICENSE-CC-BY-SA](LICENSE-CC-BY-SA) or
[https://creativecommons.org/licenses/by-sa/4.0/legalcode][L3]).

[L3]: https://creativecommons.org/licenses/by-sa/4.0/legalcode

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
licensed as above, without any additional terms or conditions.
