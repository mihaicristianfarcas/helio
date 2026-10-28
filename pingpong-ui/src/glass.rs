//! The window's glass: it blurs what is behind it, under the theme's tints,
//! as asides' window does (`WindowBackgroundAppearance::Blurred`).
//!
//! - macOS: GPUI puts an `NSVisualEffectView` under its view in the window's
//!   content view, blurring the whole window. Exactly asides': no mask (its
//!   `native::set_glass` keeps the blur to the pill's shapes, which a window
//!   of pages has none of), and GPUI's alpha blending made "over"
//!   ([`blend_alpha_over`]).
//! - Windows: GPUI's acrylic (`SetWindowCompositionAttribute`, Windows 10
//!   1809 and later), the nearest Windows has to blur behind a window. Mica
//!   is Windows 11's own material, but it tints with the wallpaper rather
//!   than blurring what is behind.
//! - Linux: GPUI blurs through KDE's `org_kde_kwin_blur`, under Wayland,
//!   where the compositor offers it: KWin does while it blurs (its blur
//!   effect on, compositing with OpenGL), not when it draws with QPainter
//!   (no GPU). Elsewhere (GNOME, X11, KWin without blur) a "blurred" window
//!   would just be see-through, so it is opaque and the theme's surfaces
//!   are solid. Whether it is offered is asked of the compositor once.
//!
//! No GPUI shadows on glass: under a translucent element a shadow fills
//! under it too, and turned it near black (asides' AGENTS.md). Shadows stay
//! on what is solid (menus, tooltips, a switch's knob).

use gpui::WindowBackgroundAppearance;

/// Whether this system's windows are glass (see the module's words).
pub fn supported() -> bool {
    #[cfg(target_os = "linux")]
    {
        static BLURS: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *BLURS.get_or_init(|| {
            let blurs = compositor_blurs();
            tracing::info!(glass = blurs, "the compositor's blur");
            blurs
        })
    }
    #[cfg(not(target_os = "linux"))]
    true
}

/// Whether the Wayland compositor GPUI draws through offers KDE's blur
/// (`org_kde_kwin_blur_manager`). Not under X11: GPUI takes Wayland when
/// there is one.
#[cfg(target_os = "linux")]
fn compositor_blurs() -> bool {
    use wayland_client::protocol::wl_registry::{self, WlRegistry};
    use wayland_client::{Connection, Dispatch, QueueHandle};

    struct Offered(bool);
    impl Dispatch<WlRegistry, ()> for Offered {
        fn event(
            offered: &mut Self,
            _: &WlRegistry,
            event: wl_registry::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            if let wl_registry::Event::Global { interface, .. } = event {
                offered.0 |= interface == "org_kde_kwin_blur_manager";
            }
        }
    }

    if std::env::var_os("WAYLAND_DISPLAY").is_none_or(|v| v.is_empty()) {
        return false;
    }
    let Ok(connection) = Connection::connect_to_env() else {
        return false;
    };
    let mut queue = connection.new_event_queue();
    let _registry = connection.display().get_registry(&queue.handle(), ());
    let mut offered = Offered(false);
    // One round trip: the registry has said every global by its end.
    queue.roundtrip(&mut offered).is_ok() && offered.0
}

/// What a main window's background is: glass where [`supported`].
pub fn window_background() -> WindowBackgroundAppearance {
    if supported() {
        WindowBackgroundAppearance::Blurred
    } else {
        WindowBackgroundAppearance::Opaque
    }
}

/// GPUI blends alpha by adding it (its pipelines' destination alpha factor is
/// one): what is partly drawn over the window's glass (a menu or a page
/// fading in) hid the blur behind it before its colour showed, a dark square
/// for a moment. This makes the pipelines GPUI asks for blend alpha as they
/// blend colour ("over"), on the device GPUI draws with: call it before GPUI
/// opens a window. It is asides' `native::blend_alpha_over`.
///
/// It changes only pipelines that blend colour "over" and add alpha. The
/// stream's renderer (`ping_core::mac::render`) draws video without blending
/// and its overlays blend alpha "over" already, so its pipelines are never
/// changed; and none of this runs as a frame is presented.
#[cfg(target_os = "macos")]
pub fn blend_alpha_over() {
    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, Imp, Sel};
    use std::sync::OnceLock;

    type NewPipeline = unsafe extern "C-unwind" fn(
        &AnyObject,
        Sel,
        *mut AnyObject,
        *mut *mut AnyObject,
    ) -> *mut AnyObject;
    static ORIGINAL: OnceLock<NewPipeline> = OnceLock::new();
    // MTLBlendFactor.
    const ONE: usize = 1;
    const ONE_MINUS_SOURCE_ALPHA: usize = 5;

    unsafe extern "C-unwind" fn new_pipeline(
        device: &AnyObject,
        cmd: Sel,
        descriptor: *mut AnyObject,
        error: *mut *mut AnyObject,
    ) -> *mut AnyObject {
        // SAFETY: `descriptor` is the MTLRenderPipelineDescriptor Metal was
        // handed (or null); its first colour attachment is an
        // MTLRenderPipelineColorAttachmentDescriptor, and these are its
        // documented getters and setter.
        unsafe {
            if let Some(descriptor) = descriptor.as_ref() {
                let attachments: *mut AnyObject = msg_send![descriptor, colorAttachments];
                let color: *mut AnyObject = match attachments.as_ref() {
                    Some(a) => msg_send![a, objectAtIndexedSubscript: 0usize],
                    None => std::ptr::null_mut(),
                };
                if let Some(color) = color.as_ref() {
                    let blending: bool = msg_send![color, isBlendingEnabled];
                    let rgb: usize = msg_send![color, destinationRGBBlendFactor];
                    let alpha: usize = msg_send![color, destinationAlphaBlendFactor];
                    if blending && rgb == ONE_MINUS_SOURCE_ALPHA && alpha == ONE {
                        let _: () = msg_send![
                            color,
                            setDestinationAlphaBlendFactor: ONE_MINUS_SOURCE_ALPHA
                        ];
                    }
                }
            }
        }
        match ORIGINAL.get() {
            // SAFETY: the method's own implementation, called with the
            // arguments it was given.
            Some(original) => unsafe { original(device, cmd, descriptor, error) },
            None => std::ptr::null_mut(),
        }
    }

    #[link(name = "Metal", kind = "framework")]
    extern "C" {
        fn MTLCopyAllDevices() -> *mut AnyObject;
    }
    if ORIGINAL.get().is_some() {
        return;
    }
    // SAFETY: MTLCopyAllDevices returns a retained NSArray (or null).
    let devices: Option<Retained<AnyObject>> = unsafe { Retained::from_raw(MTLCopyAllDevices()) };
    let Some(devices) = devices else { return };
    // SAFETY: NSArray's `count` and `objectAtIndex:` within its bounds; each
    // element is an MTLDevice, whose `isRemovable` and `isLowPower` are BOOLs.
    let device = unsafe {
        let count: usize = msg_send![&*devices, count];
        // GPUI's device: built in, and not the low-power one when there are two.
        (0..count)
            .map(|i| -> &AnyObject { msg_send![&*devices, objectAtIndex: i] })
            .max_by_key(|d| {
                let removable: bool = msg_send![*d, isRemovable];
                let low_power: bool = msg_send![*d, isLowPower];
                (!removable, !low_power)
            })
    };
    let Some(device) = device else { return };
    let Some(method) = device
        .class()
        .instance_method(objc2::sel!(newRenderPipelineStateWithDescriptor:error:))
    else {
        return;
    };
    // SAFETY: `new_pipeline` has the method's signature; the original
    // implementation is kept and called by it.
    unsafe {
        let ours: Imp = std::mem::transmute::<NewPipeline, Imp>(new_pipeline);
        let original = method.set_implementation(ours);
        let _ = ORIGINAL.set(std::mem::transmute::<Imp, NewPipeline>(original));
    }
}

/// Elsewhere GPUI's pipelines blend as the system composites them.
#[cfg(not(target_os = "macos"))]
pub fn blend_alpha_over() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glass_windows_blur_and_others_are_opaque() {
        let expected = if supported() {
            WindowBackgroundAppearance::Blurred
        } else {
            WindowBackgroundAppearance::Opaque
        };
        assert_eq!(window_background(), expected);
        if cfg!(any(target_os = "macos", windows)) {
            assert!(supported());
        }
    }
}
