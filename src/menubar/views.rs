//! Custom NSView classes: a non-drawing flipped container, a drawn shape and the popover root
//! that turns Escape into Back / close.

use super::*;

define_class!(
    // SAFETY: NSView subclass with no extra requirements; `Plain` does not implement Drop.
    #[unsafe(super(NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "HostwatchPlainView"]
    pub(super) struct Plain;

    impl Plain {
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }
    }
);

impl Plain {
    /// A non-drawing flipped container, so it gets no backing store of its own.
    pub(super) fn new(mtm: MainThreadMarker, frame: NSRect) -> Retained<Self> {
        unsafe { msg_send![Self::alloc(mtm), initWithFrame: frame] }
    }
}

#[derive(Clone, Debug)]
pub(super) enum Shape {
    /// Rounded group background; a tone adds a status-colored border.
    Card(Option<Tone>),
    Chart(Box<Chart>, bool),
    Bar(f64, Tone),
    Rule,
    Boxes(u32, u32),
}

define_class!(
    // SAFETY: NSView subclass with no extra requirements; `ShapeView` does not implement Drop.
    #[unsafe(super(NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "HostwatchShapeView"]
    #[ivars = RefCell<Shape>]
    pub(super) struct ShapeView;

    impl ShapeView {
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }

        // SAFETY: signature matches `-[NSView drawRect:]`.
        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty: NSRect) {
            draw(&self.ivars().borrow(), self.bounds());
        }
    }
);

impl ShapeView {
    pub(super) fn new(mtm: MainThreadMarker, shape: Shape, frame: NSRect) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(RefCell::new(shape));
        unsafe { msg_send![super(this), initWithFrame: frame] }
    }
}

pub(super) struct RootIvars {
    pub(super) controller: OnceCell<Weak<Controller>>,
}

define_class!(
    // SAFETY: NSView subclass with no extra requirements; `RootView` does not implement Drop.
    #[unsafe(super(NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "HostwatchRootView"]
    #[ivars = RootIvars]
    pub(super) struct RootView;

    impl RootView {
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }

        #[unsafe(method(acceptsFirstResponder))]
        fn accepts_first_responder(&self) -> bool {
            true
        }

        // SAFETY: Escape is delivered as `cancelOperation:` through the responder chain.
        #[unsafe(method(cancelOperation:))]
        fn cancel_operation(&self, _sender: Option<&AnyObject>) {
            if let Some(controller) = self.ivars().controller.get().and_then(Weak::load) {
                controller.escape();
            }
        }
    }
);
