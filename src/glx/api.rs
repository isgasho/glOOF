use libc::{c_char, c_int, c_uchar, free, malloc};
use std::ffi::{CStr, CString};
use std::sync::Arc;
use std::{iter, mem, ptr};
use x11::glx::{GLXContext, GLXDrawable};
use x11::xlib::{
    Bool, Display, False, InputOutput, True, XDefaultVisual, XFetchName, XMatchVisualInfo,
    XStoreName, XVisualInfo,
};

macro_rules! visual_attribs {
    (@type bool) => {bool};
    (@type int) => {i32};
    (@parse($next:expr) $name:ident: bool) => {Self::$name(true)};
    (@parse($next:expr) $name:ident: int) => {Self::$name($next())};
    ($($name:ident: $ty:ident),* $(,)?) => {
        #[allow(non_camel_case_types)]
        #[derive(Copy, Clone, Debug)]
        enum VisualAttrib {
            $($name(visual_attribs!(@type $ty))),*
        }
        impl VisualAttrib {
            unsafe fn parse_list(mut list: *mut c_int) -> impl Iterator<Item = Self> {
                iter::from_fn(move || {
                    if *list == 0 {
                        return None;
                    }
                    let mut next = || {
                        let attrib = *list;
                        list = list.add(1);
                        attrib
                    };
                    Some(match next() {
                        $(x11::glx::$name => visual_attribs!(@parse(next) $name: $ty),)*
                        attrib => panic!("VisualAttrib::parse_list: invalid attribute {}", attrib),
                    })
                })

            }
        }

        #[allow(non_snake_case)]
        #[derive(Copy, Clone, Debug)]
        struct VisualAttribs {
            $($name: visual_attribs!(@type $ty)),*
        }
        impl Default for VisualAttribs {
            fn default() -> Self {
                Self {
                    GLX_USE_GL: true,
                    ..Self {
                        $($name: Default::default()),*
                    }
                }
            }
        }
        impl Extend<VisualAttrib> for VisualAttribs {
            fn extend<I: IntoIterator<Item = VisualAttrib>>(&mut self, iter: I) {
                for attrib in iter {
                    match attrib {
                        $(VisualAttrib::$name(x) => self.$name = x),*
                    }
                }
            }
        }
        impl iter::FromIterator<VisualAttrib> for VisualAttribs {
            fn from_iter<I: IntoIterator<Item = VisualAttrib>>(iter: I) -> Self {
                let mut attribs = Self::default();
                attribs.extend(iter);
                attribs
            }
        }
    };
}

visual_attribs! {
    GLX_USE_GL: bool,
    GLX_BUFFER_SIZE: int,
    GLX_LEVEL: int,
    GLX_RGBA: bool,
    GLX_DOUBLEBUFFER: bool,
    GLX_STEREO: bool,
    GLX_AUX_BUFFERS: int,
    GLX_RED_SIZE: int,
    GLX_GREEN_SIZE: int,
    GLX_BLUE_SIZE: int,
    GLX_ALPHA_SIZE: int,
    GLX_DEPTH_SIZE: int,
    GLX_STENCIL_SIZE: int,
    GLX_ACCUM_RED_SIZE: int,
    GLX_ACCUM_GREEN_SIZE: int,
    GLX_ACCUM_BLUE_SIZE: int,
    GLX_ACCUM_ALPHA_SIZE: int,
    GLX_FBCONFIG_ID: int,
}

#[no_mangle]
pub unsafe extern "C" fn glXChooseVisual(
    dpy: *mut Display,
    screen: c_int,
    attrib_list: *mut c_int,
) -> *mut XVisualInfo {
    assert_eq!(screen, 0);

    eprintln!("glXChooseVisual(attribList=[");
    for attrib in VisualAttrib::parse_list(attrib_list) {
        eprintln!("    {:?},", attrib);
    }
    eprintln!("])");

    eprintln!(
        "attribs = {:#?}",
        VisualAttrib::parse_list(attrib_list).collect::<VisualAttribs>()
    );

    let visual_info = malloc(mem::size_of::<XVisualInfo>()) as *mut XVisualInfo;

    // HACK(eddyb) `XMatchVisualInfo` returns 0 (failure) for some reason.
    if false {
        if XMatchVisualInfo(dpy, 0, 24, InputOutput, visual_info) == 0 {
            free(visual_info as *mut _);
            return ptr::null_mut();
        }
        eprintln!("visual_info = {:#?}", *visual_info);
    } else {
        visual_info.write(XVisualInfo {
            visual: XDefaultVisual(dpy, 0),
            visualid: 0,
            screen: 0,
            depth: 0,
            class: 0,
            red_mask: 0,
            green_mask: 0,
            blue_mask: 0,
            colormap_size: 0,
            bits_per_rgb: 0,
        });
    }
    visual_info
}

#[no_mangle]
pub unsafe extern "C" fn glXCreateContext(
    _dpy: *mut Display,
    vis: *mut XVisualInfo,
    share_list: GLXContext,
    direct: Bool,
) -> GLXContext {
    assert_eq!(share_list, ptr::null_mut());

    eprintln!(
        "glXCreateContext(vis={:#?}, direct={})",
        *vis,
        direct != False
    );

    Arc::into_raw(super::Context::new()) as GLXContext
}

#[no_mangle]
pub unsafe extern "C" fn glXMakeCurrent(
    dpy: *mut Display,
    drawable: GLXDrawable,
    ctx: GLXContext,
) -> Bool {
    eprintln!("glXMakeCurrent(drawable={:#x}, ctx={:#?})", drawable, ctx);

    if ctx.is_null() {
        super::Context::remove_current();
    } else {
        mem::ManuallyDrop::new(Arc::from_raw(ctx as *mut super::Context))
            .make_current(super::Surface { drawable }, super::Surface { drawable });
    }

    let mut name = ptr::null_mut();
    XFetchName(dpy, drawable, &mut name);
    if !name.is_null() {
        let new_name = CString::new(format!(
            "{} [glOOF]",
            CStr::from_ptr(name).to_str().unwrap()
        ))
        .unwrap();
        XStoreName(dpy, drawable, new_name.as_ptr());
        free(name as *mut _);
    }

    True
}

#[no_mangle]
pub unsafe extern "C" fn glXQueryExtensionsString(
    _dpy: *mut Display,
    screen: c_int,
) -> *const c_char {
    assert_eq!(screen, 0);

    eprintln!("glXQueryExtensionsString()");

    b"\0" as *const _ as *const c_char
}

#[no_mangle]
pub unsafe extern "C" fn glXGetProcAddressARB(
    proc_name: *const c_uchar,
) -> Option<unsafe extern "C" fn()> {
    let proc_name = CStr::from_ptr(proc_name as *const c_char).to_str().unwrap();

    unimplemented!("glXQueryExtensionsString({:?})", proc_name);
}

#[no_mangle]
pub unsafe extern "C" fn glXSwapBuffers(dpy: *mut Display, drawable: GLXDrawable) {
    use x11::xlib::*;

    // eprintln!("glXSwapBuffers(drawable={:#x})", drawable);

    match &*super::Context::get_current().unwrap().0.lock().unwrap() {
        super::State::Inactive(_) => unreachable!("glOOF: inactive current GLX context"),
        super::State::Current { read, draw } => {
            assert_eq!(read.drawable, drawable);
            assert_eq!(draw.drawable, drawable);
        }
    }

    XClearWindow(dpy, drawable);
    let gc = XCreateGC(
        dpy,
        drawable,
        GCForeground as u64,
        &XGCValues {
            foreground: 0xffffff,
            ..mem::zeroed()
        } as *const _ as *mut _,
    );

    crate::gl::Context::with("glXSwapBuffers", |gl| {
        let mut last_point = None;
        for cmd in gl.pending_cmds.drain(..) {
            gl.state.apply(cmd, &mut |state, cmd| {
                use crate::gl::api_1_0::Command::*;
                use glam::Vec3;
                match cmd {
                    glVertex3f(x, y, z) => {
                        let mut p = Vec3::new(x, y, z);
                        p = state.modelview.mat.transform_point3(p);
                        p = state.projection.mat.transform_point3(p);
                        match last_point {
                            None => last_point = Some(p),
                            Some(last) => {
                                XDrawLine(
                                    dpy,
                                    drawable,
                                    gc,
                                    (last.x * 300.0 / 2.0 + 300.0 / 2.0) as _,
                                    (-last.y * 300.0 / 2.0 + 300.0 / 2.0) as _,
                                    (p.x * 300.0 / 2.0 + 300.0 / 2.0) as _,
                                    (-p.y * 300.0 / 2.0 + 300.0 / 2.0) as _,
                                );
                            }
                        }
                    }
                    _ => {
                        // eprintln!("{:?}", cmd);
                    }
                }
            });
        }
    });

    XFlush(dpy);
}
