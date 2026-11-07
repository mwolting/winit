//! A main-thread display link, used by the Apple window backends.
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol};
use objc2::{declare_class, msg_send_id, mutability, sel, ClassType, DeclaredClass};
use objc2_foundation::{MainThreadMarker, NSRunLoop, NSRunLoopCommonModes};
use objc2_quartz_core::{CADisplayLink, CAFrameRateRange};

declare_class!(
    struct DisplayLinkTarget;

    unsafe impl ClassType for DisplayLinkTarget {
        type Super = NSObject;
        type Mutability = mutability::MainThreadOnly;
        const NAME: &'static str = "WinitDisplayLinkTarget";
    }

    impl DeclaredClass for DisplayLinkTarget {
        type Ivars = Box<dyn Fn()>;
    }

    unsafe impl NSObjectProtocol for DisplayLinkTarget {}

    unsafe impl DisplayLinkTarget {
        #[method(displayLinkTick:)]
        fn tick(&self, _link: &CADisplayLink) {
            (self.ivars())();
        }
    }
);

pub(crate) struct DisplayLink(Retained<CADisplayLink>);

impl DisplayLink {
    /// `source` is an NSWindow on macOS; iOS creates its link via CADisplayLink.
    pub(crate) fn new(
        mtm: MainThreadMarker,
        source: &AnyObject,
        preferred_fps: u32,
        callback: impl Fn() + 'static,
    ) -> Option<Self> {
        let target = mtm.alloc::<DisplayLinkTarget>().set_ivars(Box::new(callback));
        // SAFETY: NSObject is the target's declared superclass.
        let target: Retained<DisplayLinkTarget> = unsafe { msg_send_id![super(target), init] };
        #[cfg(macos_platform)]
        let link: Retained<CADisplayLink> = unsafe {
            let available: bool =
                objc2::msg_send![source, respondsToSelector: sel!(displayLinkWithTarget:selector:)];
            if !available {
                return None;
            }
            msg_send_id![source, displayLinkWithTarget: &*target, selector: sel!(displayLinkTick:)]
        };
        #[cfg(ios_platform)]
        let link = unsafe {
            let _ = source;
            CADisplayLink::displayLinkWithTarget_selector(&target, sel!(displayLinkTick:))
        };
        // Check availability: winit supports OS versions predating the frame-rate-range API.
        let supports_range: bool = unsafe {
            objc2::msg_send![&*link, respondsToSelector: sel!(setPreferredFrameRateRange:)]
        };
        if !supports_range {
            return None;
        }
        let preferred = preferred_fps.max(1) as f32;
        // SAFETY: created and used on the main thread, with a selector implemented above.
        unsafe {
            link.setPreferredFrameRateRange(CAFrameRateRange {
                minimum: 1.0,
                maximum: preferred,
                preferred,
            });
            link.addToRunLoop_forMode(&NSRunLoop::currentRunLoop(), NSRunLoopCommonModes);
        }
        Some(Self(link))
    }

    pub(crate) fn set_paused(&self, paused: bool) {
        unsafe { self.0.setPaused(paused) }
    }
}

impl Drop for DisplayLink {
    fn drop(&mut self) {
        unsafe { self.0.invalidate() }
    }
}
