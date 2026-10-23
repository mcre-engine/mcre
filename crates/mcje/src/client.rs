//! Client-side control for the embedded vanilla client.
//!
//! The client's windowing stack (SDL3/AppKit on macOS) requires the process
//! main thread, so [`run_client_blocking`] must be called directly from
//! `main`; a controller thread drives the game through [`ClientControl`] while
//! the client runs.

use std::time::{Duration, Instant};

use jni::objects::{JObject, JValue};
use jni::{JavaVM, jni_sig, jni_str};

use crate::find_thread_by_name;

const RENDER_THREAD_NAME: &str = "Render thread";

/// Camera state read back from the running vanilla client.
#[derive(Clone, Copy, Debug)]
pub struct PlayerState {
    /// Absolute eye position (feet + eye height).
    pub eye: [f64; 3],
    /// Vanilla yaw in degrees (0 = south, positive = west).
    pub yaw: f32,
    /// Vanilla pitch in degrees (positive = down).
    pub pitch: f32,
    /// Actual standing eye height reported by the entity.
    pub eye_height: f32,
    /// Effective vertical field of view in degrees, after every modifier the
    /// client applies (flying, speed, effects).
    pub fov: f32,
}

/// Run `net.minecraft.client.main.Main` on the current thread.
///
/// Blocks until the client exits. On macOS the client must own the process
/// main thread, so call this from `main` and drive the game from a separate
/// controller thread via [`ClientControl`].
pub fn run_client_blocking(jvm: &JavaVM, args: &[String]) -> Result<(), jni::errors::Error> {
    jvm.attach_current_thread(|env| {
        let string_class = env.find_class(jni_str!("java/lang/String"))?;
        let array = env.new_object_array(args.len() as i32, &string_class, JObject::null())?;
        for (index, arg) in args.iter().enumerate() {
            let value = env.new_string(arg)?;
            array.set_element(env, index, &value)?;
        }
        let array = JObject::from(array);
        env.call_static_method(
            jni_str!("net/minecraft/client/main/Main"),
            jni_str!("main"),
            jni_sig!("([Ljava/lang/String;)V"),
            &[JValue::Object(&array)],
        )?;
        Ok::<_, jni::errors::Error>(())
    })
}

/// Handle to a running vanilla client, usable from any attached thread.
#[derive(Clone)]
pub struct ClientControl {
    jvm: JavaVM,
}

impl ClientControl {
    pub fn new(jvm: &JavaVM) -> Self {
        Self { jvm: jvm.clone() }
    }

    fn with_env<T>(&self, f: impl FnOnce(&mut jni::Env) -> Result<T, String>) -> Result<T, String> {
        self.jvm
            .attach_current_thread(|env| Ok::<_, jni::errors::Error>(f(env)))
            .map_err(|err| format!("JNI failure: {err}"))?
    }

    /// True while the client's render thread still exists.
    pub fn render_thread_alive(&self) -> bool {
        self.with_env(|env| Ok(find_thread_by_name(env, RENDER_THREAD_NAME).is_some()))
            .unwrap_or(false)
    }

    /// Block until the client has a level and a local player.
    pub fn wait_for_world(&self, timeout: Duration) -> Result<(), String> {
        let deadline = Instant::now() + timeout;
        let mut seen_render_thread = false;
        loop {
            if let Some(err) = self.shot_error()? {
                return Err(format!("client step failed: {err}"));
            }
            if self.with_env(client_ready)? {
                return Ok(());
            }
            if self.render_thread_alive() {
                seen_render_thread = true;
            } else if seen_render_thread {
                return Err("render thread exited before the world was ready".into());
            }
            if Instant::now() >= deadline {
                return Err(format!("world not ready after {timeout:?}"));
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    /// Read the camera state (eye, rotation, eye height) from the client.
    pub fn player_state(&self) -> Result<PlayerState, String> {
        self.with_env(|env| {
            let mc = instance_required(env)?;
            let player = field_object(
                env,
                &mc,
                jni_str!("player"),
                jni_sig!("Lnet/minecraft/client/player/LocalPlayer;"),
            )?;
            let eye_height = call_f(env, &player, jni_str!("getEyeHeight"), jni_sig!("()F"))?;
            let x = call_d(env, &player, jni_str!("getX"), jni_sig!("()D"))?;
            let y = call_d(env, &player, jni_str!("getY"), jni_sig!("()D"))?;
            let z = call_d(env, &player, jni_str!("getZ"), jni_sig!("()D"))?;
            let yaw = call_f(env, &player, jni_str!("getYRot"), jni_sig!("()F"))?;
            let pitch = call_f(env, &player, jni_str!("getXRot"), jni_sig!("()F"))?;
            let renderer = field_object(
                env,
                &mc,
                jni_str!("gameRenderer"),
                jni_sig!("Lnet/minecraft/client/renderer/GameRenderer;"),
            )?;
            let camera = field_object(
                env,
                &renderer,
                jni_str!("mainCamera"),
                jni_sig!("Lnet/minecraft/client/Camera;"),
            )?;
            let fov = call_f(env, &camera, jni_str!("getFov"), jni_sig!("()F"))?;
            Ok(PlayerState {
                eye: [x, y + f64::from(eye_height), z],
                yaw,
                pitch,
                eye_height,
                fov,
            })
        })
    }

    /// Overworld day clock as synced to the client.
    pub fn overworld_clock_time(&self) -> Result<i64, String> {
        self.with_env(|env| {
            let mc = instance_required(env)?;
            let level = field_object(
                env,
                &mc,
                jni_str!("level"),
                jni_sig!("Lnet/minecraft/client/multiplayer/ClientLevel;"),
            )?;
            call_j(
                env,
                &level,
                jni_str!("getOverworldClockTime"),
                jni_sig!("()J"),
            )
        })
    }

    /// Framebuffer size in physical pixels.
    pub fn framebuffer_size(&self) -> Result<(i32, i32), String> {
        self.with_env(|env| {
            let mc = instance_required(env)?;
            let window = call_window(env, &mc)?;
            let width = call_i(env, &window, jni_str!("getWidth"), jni_sig!("()I"))?;
            let height = call_i(env, &window, jni_str!("getHeight"), jni_sig!("()I"))?;
            Ok((width, height))
        })
    }

    /// Resize the window in logical units; the framebuffer should follow.
    pub fn set_windowed(&self, width: i32, height: i32) -> Result<(), String> {
        self.with_env(|env| {
            let mc = instance_required(env)?;
            let window = call_window(env, &mc)?;
            env.call_method(
                &window,
                jni_str!("setWindowed"),
                jni_sig!("(II)V"),
                &[JValue::Int(width), JValue::Int(height)],
            )
            .map_err(|err| format!("Window.setWindowed failed: {err}"))?;
            Ok(())
        })
    }

    /// Hide the HUD (no-op if already hidden).
    pub fn schedule_hide(&self) -> Result<(), String> {
        self.schedule_step("hide", None)
    }

    /// Screenshot the main render target to `screenshots/<name>`.
    pub fn schedule_grab(&self, name: &str) -> Result<(), String> {
        self.schedule_step("grab", Some(name))
    }

    /// Teleport the server player to `feet` with a rotation (server thread).
    pub fn schedule_fix(&self, feet: [f64; 3], yaw: f32, pitch: f32) -> Result<(), String> {
        self.with_env(|env| {
            let mc = instance_required(env)?;
            let driver = env
                .new_object(
                    jni_str!("mcre/ShotDriver"),
                    jni_sig!("(DDDFF)V"),
                    &[
                        JValue::Double(feet[0]),
                        JValue::Double(feet[1]),
                        JValue::Double(feet[2]),
                        JValue::Float(yaw),
                        JValue::Float(pitch),
                    ],
                )
                .map_err(|err| format!("ShotDriver construct failed: {err}"))?;
            execute(env, &mc, &driver)
        })
    }

    /// Last error recorded by a `ShotDriver` step, if any.
    pub fn shot_error(&self) -> Result<Option<String>, String> {
        self.with_env(|env| {
            let class = env
                .find_class(jni_str!("mcre/ShotDriver"))
                .map_err(|err| format!("find mcre/ShotDriver failed: {err}"))?;
            let value = env
                .get_static_field(&class, jni_str!("error"), jni_sig!("Ljava/lang/String;"))
                .map_err(|err| format!("ShotDriver.error read failed: {err}"))?
                .l()
                .map_err(|err| format!("ShotDriver.error read failed: {err}"))?;
            if value.is_null() {
                return Ok(None);
            }
            let text = env
                .as_cast::<jni::objects::JString>(&value)
                .map_err(|err| format!("ShotDriver.error decode failed: {err}"))?
                .try_to_string(env)
                .map_err(|err| format!("ShotDriver.error decode failed: {err}"))?;
            Ok(Some(text))
        })
    }

    /// Ask the client to stop (saves the world, exits the render loop).
    pub fn stop(&self) -> Result<(), String> {
        self.with_env(|env| {
            let Some(mc) = instance(env)? else {
                return Ok(());
            };
            env.call_method(&mc, jni_str!("stop"), jni_sig!("()V"), &[])
                .map_err(|err| format!("Minecraft.stop failed: {err}"))?;
            Ok(())
        })
    }

    fn schedule_step(&self, step: &str, arg: Option<&str>) -> Result<(), String> {
        self.with_env(|env| {
            let mc = instance_required(env)?;
            let step = JObject::from(
                env.new_string(step)
                    .map_err(|err| format!("ShotDriver step encode failed: {err}"))?,
            );
            let arg = match arg {
                Some(arg) => JObject::from(
                    env.new_string(arg)
                        .map_err(|err| format!("ShotDriver arg encode failed: {err}"))?,
                ),
                None => JObject::null(),
            };
            let driver = env
                .new_object(
                    jni_str!("mcre/ShotDriver"),
                    jni_sig!("(Ljava/lang/String;Ljava/lang/String;)V"),
                    &[JValue::Object(&step), JValue::Object(&arg)],
                )
                .map_err(|err| format!("ShotDriver construct failed: {err}"))?;
            execute(env, &mc, &driver)
        })
    }
}

fn client_ready(env: &mut jni::Env) -> Result<bool, String> {
    let Some(mc) = instance(env)? else {
        return Ok(false);
    };
    let level = field_object(
        env,
        &mc,
        jni_str!("level"),
        jni_sig!("Lnet/minecraft/client/multiplayer/ClientLevel;"),
    )?;
    let player = field_object(
        env,
        &mc,
        jni_str!("player"),
        jni_sig!("Lnet/minecraft/client/player/LocalPlayer;"),
    )?;
    Ok(!level.is_null() && !player.is_null())
}

/// The `Minecraft` singleton, or `None` while the client is still starting up
/// (the constructor assigns it after `getInstance` starts returning values).
fn instance<'local>(env: &mut jni::Env<'local>) -> Result<Option<JObject<'local>>, String> {
    let obj = env
        .call_static_method(
            jni_str!("net/minecraft/client/Minecraft"),
            jni_str!("getInstance"),
            jni_sig!("()Lnet/minecraft/client/Minecraft;"),
            &[],
        )
        .map_err(|err| format!("Minecraft.getInstance failed: {err}"))?
        .l()
        .map_err(|err| format!("Minecraft.getInstance failed: {err}"))?;
    Ok(if obj.is_null() { None } else { Some(obj) })
}

/// Like [`instance`], but errors if the client has not finished starting.
fn instance_required<'local>(env: &mut jni::Env<'local>) -> Result<JObject<'local>, String> {
    instance(env)?.ok_or_else(|| "Minecraft is still starting up".into())
}

fn field_object<'local>(
    env: &mut jni::Env<'local>,
    obj: &JObject<'local>,
    name: &jni::strings::JNIStr,
    sig: jni::signature::FieldSignature,
) -> Result<JObject<'local>, String> {
    env.get_field(obj, name, sig)
        .map_err(|err| format!("field {name} read failed: {err}"))?
        .l()
        .map_err(|err| format!("field {name} read failed: {err}"))
}

fn call_f(
    env: &mut jni::Env,
    obj: &JObject,
    name: &jni::strings::JNIStr,
    sig: jni::signature::MethodSignature,
) -> Result<f32, String> {
    env.call_method(obj, name, sig, &[])
        .map_err(|err| format!("call failed: {err}"))?
        .f()
        .map_err(|err| format!("call failed: {err}"))
}

fn call_d(
    env: &mut jni::Env,
    obj: &JObject,
    name: &jni::strings::JNIStr,
    sig: jni::signature::MethodSignature,
) -> Result<f64, String> {
    env.call_method(obj, name, sig, &[])
        .map_err(|err| format!("call failed: {err}"))?
        .d()
        .map_err(|err| format!("call failed: {err}"))
}

fn call_i(
    env: &mut jni::Env,
    obj: &JObject,
    name: &jni::strings::JNIStr,
    sig: jni::signature::MethodSignature,
) -> Result<i32, String> {
    env.call_method(obj, name, sig, &[])
        .map_err(|err| format!("call failed: {err}"))?
        .i()
        .map_err(|err| format!("call failed: {err}"))
}

fn call_j(
    env: &mut jni::Env,
    obj: &JObject,
    name: &jni::strings::JNIStr,
    sig: jni::signature::MethodSignature,
) -> Result<i64, String> {
    env.call_method(obj, name, sig, &[])
        .map_err(|err| format!("call failed: {err}"))?
        .j()
        .map_err(|err| format!("call failed: {err}"))
}

fn call_window<'local>(
    env: &mut jni::Env<'local>,
    mc: &JObject<'local>,
) -> Result<JObject<'local>, String> {
    env.call_method(
        mc,
        jni_str!("getWindow"),
        jni_sig!("()Lcom/mojang/blaze3d/platform/Window;"),
        &[],
    )
    .map_err(|err| format!("Minecraft.getWindow failed: {err}"))?
    .l()
    .map_err(|err| format!("Minecraft.getWindow returned null: {err}"))
}

fn execute(env: &mut jni::Env, mc: &JObject, driver: &JObject) -> Result<(), String> {
    env.call_method(
        mc,
        jni_str!("execute"),
        jni_sig!("(Ljava/lang/Runnable;)V"),
        &[JValue::Object(driver)],
    )
    .map_err(|err| format!("Minecraft.execute failed: {err}"))?;
    Ok(())
}
