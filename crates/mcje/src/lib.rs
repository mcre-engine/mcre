use jni::{
    InitArgsBuilder, JNIEnv, JavaVM,
    objects::{JObject, JValueGen},
};

pub use mcje_macros::*;

pub async fn init() -> JavaVM {
    let classpath = env!("MCJE_JVM_CLASSPATH");

    let jvm_args = InitArgsBuilder::new()
        .option(format!("-Djava.class.path={classpath}"))
        .build()
        .unwrap();

    JavaVM::new(jvm_args).unwrap()
}

pub fn bootstrap(env: &mut JNIEnv) {
    let detected_version_built_in = env
        .get_static_field(
            "net/minecraft/DetectedVersion",
            "BUILT_IN",
            "Lnet/minecraft/WorldVersion;",
        )
        .unwrap()
        .l()
        .unwrap();

    env.call_static_method(
        "net/minecraft/SharedConstants",
        "setVersion",
        "(Lnet/minecraft/WorldVersion;)V",
        &[JValueGen::Object(&detected_version_built_in)],
    )
    .unwrap();

    env.call_static_method("net/minecraft/server/Bootstrap", "bootStrap", "()V", &[])
        .unwrap();
}

pub fn get_registry<'a>(env: &mut JNIEnv<'a>, name: &str, jtype: &str) -> JObject<'a> {
    let built_in_registries = env
        .find_class("net/minecraft/core/registries/BuiltInRegistries")
        .unwrap();

    env.get_static_field(
        built_in_registries,
        name,
        format!("Lnet/minecraft/core/{jtype};"),
    )
    .unwrap()
    .l()
    .unwrap()
}

pub fn iterate<'a>(
    obj: &JObject<'a>,
    env: &'a mut JNIEnv,
    mut cb: impl FnMut(usize, JObject<'a>, &mut JNIEnv),
) {
    let iterator = env
        .call_method(obj, "iterator", "()Ljava/util/Iterator;", &[])
        .unwrap()
        .l()
        .unwrap();

    let mut i = 0;
    loop {
        // call hasNext()
        let has_next = env
            .call_method(&iterator, "hasNext", "()Z", &[])
            .unwrap()
            .z()
            .unwrap();
        if !has_next {
            break;
        }

        // call next()
        let element = env
            .call_method(&iterator, "next", "()Ljava/lang/Object;", &[])
            .unwrap()
            .l()
            .unwrap();

        cb(i, element, env);

        i += 1;
    }
}
