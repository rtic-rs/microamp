#![no_main]
#![no_std]

use core::{
    panic::PanicInfo,
    sync::atomic::{AtomicBool, Ordering},
};

use microamp::shared;

#[shared]
static X: AtomicBool = AtomicBool::new(false);

#[shared]
static mut Y: u32 = 0;

#[allow(dead_code)]
fn main() {
    if cfg!(core = "0") {
        unsafe { Y += 1 }
    } else {
        unsafe { Y += 2 }
    }

    X.store(true, Ordering::Release);
}

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    loop {}
}
