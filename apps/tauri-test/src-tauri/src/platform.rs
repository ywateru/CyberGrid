use cybergrid_sdk::CloudflaredConfig;
use std::path::Path;
#[cfg(not(target_os = "android"))]
use std::path::PathBuf;

#[cfg(not(target_os = "android"))]
pub fn provider_runtime(configured: &str, cache_dir: &Path) -> Result<CloudflaredConfig, String> {
    if configured.is_empty() {
        return CloudflaredConfig::bundled(cache_dir)
            .map_err(|_| "SDK 内置 cloudflared 资源校验或提取失败".to_string());
    }
    let binary = PathBuf::from(configured);
    if !binary.is_absolute() || !binary.is_file() {
        return Err("外部 cloudflared 必须是可信已安装文件的绝对路径".into());
    }
    let mut provider = CloudflaredConfig::new(binary);
    provider.temp_dir = Some(cache_dir.to_path_buf());
    Ok(provider)
}

#[cfg(target_os = "android")]
pub fn provider_runtime(_configured: &str, cache_dir: &Path) -> Result<CloudflaredConfig, String> {
    use jni::{
        objects::{JObject, JString, JValue},
        JavaVM,
    };
    let context = ndk_context::android_context();
    let vm = unsafe { JavaVM::from_raw(context.vm().cast()) }
        .map_err(|_| "无法连接 Android runtime".to_string())?;
    let mut env = vm
        .attach_current_thread()
        .map_err(|_| "无法连接 Android runtime".to_string())?;
    let activity = unsafe { JObject::from_raw(context.context().cast()) };
    let result = (|| -> jni::errors::Result<(String, String)> {
        let info = env
            .call_method(
                &activity,
                "getApplicationInfo",
                "()Landroid/content/pm/ApplicationInfo;",
                &[],
            )?
            .l()?;
        let native_dir = env
            .get_field(&info, "nativeLibraryDir", "Ljava/lang/String;")?
            .l()?;
        let native_dir: String = env.get_string(&JString::from(native_dir))?.into();
        let service = env.new_string("connectivity")?;
        let service_object = JObject::from(service);
        let manager = env
            .call_method(
                &activity,
                "getSystemService",
                "(Ljava/lang/String;)Ljava/lang/Object;",
                &[JValue::Object(&service_object)],
            )?
            .l()?;
        let network = env
            .call_method(&manager, "getActiveNetwork", "()Landroid/net/Network;", &[])?
            .l()?;
        if network.is_null() {
            return Err(jni::errors::Error::NullPtr("active network"));
        }
        let properties = env
            .call_method(
                &manager,
                "getLinkProperties",
                "(Landroid/net/Network;)Landroid/net/LinkProperties;",
                &[JValue::Object(&network)],
            )?
            .l()?;
        if properties.is_null() {
            return Err(jni::errors::Error::NullPtr("network properties"));
        }
        let servers = env
            .call_method(&properties, "getDnsServers", "()Ljava/util/List;", &[])?
            .l()?;
        let size = env.call_method(&servers, "size", "()I", &[])?.i()?;
        if size < 1 {
            return Err(jni::errors::Error::NullPtr("network DNS"));
        }
        let server = env
            .call_method(&servers, "get", "(I)Ljava/lang/Object;", &[JValue::Int(0)])?
            .l()?;
        let address = env
            .call_method(&server, "getHostAddress", "()Ljava/lang/String;", &[])?
            .l()?;
        let address: String = env.get_string(&JString::from(address))?.into();
        Ok((native_dir, address))
    })();
    match result {
        Ok((native_dir, dns)) => {
            let ip: std::net::IpAddr = dns
                .parse()
                .map_err(|_| "Android 当前网络 DNS 不是合法 IP".to_string())?;
            let mut provider = CloudflaredConfig::android_native(native_dir, cache_dir)
                .map_err(|_| "APK 内置 cloudflared 资源校验失败".to_string())?;
            provider.env.insert(
                "CLOUDFLARED_DNS_SERVER".into(),
                std::net::SocketAddr::new(ip, 53).to_string(),
            );
            provider
                .env
                .insert("TMPDIR".into(), cache_dir.to_string_lossy().into_owned());
            Ok(provider)
        }
        Err(_) => {
            let _ = env.exception_clear();
            Err("无法读取 Android 原生库目录或当前网络 DNS，请确认网络已连接".into())
        }
    }
}
