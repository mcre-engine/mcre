use jni::{
    InitArgsBuilder, JavaVM, jni_sig, jni_str,
    objects::{JObject, JValue},
    signature::RuntimeFieldSignature,
    strings::JNIString,
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

pub fn bootstrap(env: &mut jni::Env) {
    let detected_version_built_in = env
        .get_static_field(
            jni_str!("net/minecraft/DetectedVersion"),
            jni_str!("BUILT_IN"),
            jni_sig!("Lnet/minecraft/WorldVersion;"),
        )
        .unwrap()
        .l()
        .unwrap();

    env.call_static_method(
        jni_str!("net/minecraft/SharedConstants"),
        jni_str!("setVersion"),
        jni_sig!("(Lnet/minecraft/WorldVersion;)V"),
        &[JValue::from(&detected_version_built_in)],
    )
    .unwrap();

    env.call_static_method(
        jni_str!("net/minecraft/server/Bootstrap"),
        jni_str!("bootStrap"),
        jni_sig!("()V"),
        &[],
    )
    .unwrap();
}

pub fn get_registry<'a>(env: &mut jni::Env<'a>, name: &str, jtype: &str) -> JObject<'a> {
    let built_in_registries = env
        .find_class(jni_str!("net/minecraft/core/registries/BuiltInRegistries"))
        .unwrap();

    let jni_name = JNIString::new(name);
    let runtime_sig =
        RuntimeFieldSignature::from_str(format!("Lnet/minecraft/core/{jtype};")).unwrap();
    let sig = runtime_sig.field_signature();

    env.get_static_field(built_in_registries, &jni_name, &sig)
        .unwrap()
        .l()
        .unwrap()
}

pub fn iterate<'a, 'env>(
    obj: &JObject<'a>,
    env: &'env mut jni::Env<'a>,
    mut cb: impl FnMut(usize, JObject<'a>, &mut jni::Env<'a>),
) {
    let iterator = env
        .call_method(
            obj,
            jni_str!("iterator"),
            jni_sig!("()Ljava/util/Iterator;"),
            &[],
        )
        .unwrap()
        .l()
        .unwrap();

    let mut i = 0;
    loop {
        let has_next = env
            .call_method(&iterator, jni_str!("hasNext"), jni_sig!("()Z"), &[])
            .unwrap()
            .z()
            .unwrap();
        if !has_next {
            break;
        }

        let element = env
            .call_method(
                &iterator,
                jni_str!("next"),
                jni_sig!("()Ljava/lang/Object;"),
                &[],
            )
            .unwrap()
            .l()
            .unwrap();

        cb(i, element, env);

        i += 1;
    }
}
