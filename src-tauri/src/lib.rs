use serde_json::{json, Value};
use std::{collections::HashMap, path::PathBuf, time::Duration};
use tauri::Manager;

const TOKEN: &str = "https://oauth2.googleapis.com/token";
const FILES: &str = "https://www.googleapis.com/drive/v3/files";
const NAME: &str = "budget-data.json";

fn err<E: ToString>(e: E) -> String { e.to_string() }

// مسار نسبي = داخل مجلد بيانات التطبيق، مسار كامل = زي ما هو (ويندوز)
fn resolve(app: &tauri::AppHandle, p: &str) -> Result<PathBuf, String> {
    let pb = PathBuf::from(p);
    if pb.is_absolute() { return Ok(pb); }
    Ok(app.path().app_data_dir().map_err(err)?.join(pb))
}

#[tauri::command]
fn read_file(app: tauri::AppHandle, path: String) -> Result<String, String> {
    std::fs::read_to_string(resolve(&app, &path)?).map_err(err)
}

#[tauri::command]
fn write_file(app: tauri::AppHandle, path: String, text: String) -> Result<(), String> {
    let p = resolve(&app, &path)?;
    if let Some(d) = p.parent() { std::fs::create_dir_all(d).map_err(err)?; }
    std::fs::write(p, text).map_err(err)
}

// مكالمات الـ AI من Rust عشان نتخطى قيود CORS
#[tauri::command]
async fn http_post(url: String, headers: HashMap<String, String>, body: String) -> Result<String, String> {
    let mut r = reqwest::Client::new().post(url).body(body);
    for (k, v) in headers { r = r.header(k, v); }
    r.send().await.map_err(err)?.text().await.map_err(err)
}

#[tauri::command]
async fn http_get(url: String) -> Result<String, String> {
    reqwest::get(url).await.map_err(err)?.text().await.map_err(err)
}

// ---------- Google Drive (Device Flow: بدون خدمات جوجل وبدون redirect) ----------
#[tauri::command]
async fn g_start(id: String) -> Result<String, String> {
    reqwest::Client::new()
        .post("https://oauth2.googleapis.com/device/code")
        .form(&[("client_id", id.as_str()), ("scope", "https://www.googleapis.com/auth/drive.file")])
        .send().await.map_err(err)?.text().await.map_err(err)
}

#[tauri::command]
async fn g_poll(id: String, secret: String, dc: String, iv: u64) -> Result<String, String> {
    let c = reqwest::Client::new();
    let mut wait = iv.max(5);
    for _ in 0..120 {
        tokio::time::sleep(Duration::from_secs(wait)).await;
        let v: Value = c.post(TOKEN)
            .form(&[("client_id", id.as_str()), ("client_secret", secret.as_str()), ("device_code", dc.as_str()),
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code")])
            .send().await.map_err(err)?.json().await.map_err(err)?;
        if let Some(rt) = v["refresh_token"].as_str() { return Ok(rt.to_string()); }
        match v["error"].as_str() {
            Some("authorization_pending") => {}
            Some("slow_down") => wait += 5,
            Some(e) => return Err(e.to_string()),
            None => return Err("رد غير متوقع من جوجل".into()),
        }
    }
    Err("انتهت مهلة تسجيل الدخول".into())
}

async fn access(id: &str, secret: &str, rt: &str) -> Result<String, String> {
    let v: Value = reqwest::Client::new().post(TOKEN)
        .form(&[("client_id", id), ("client_secret", secret), ("refresh_token", rt), ("grant_type", "refresh_token")])
        .send().await.map_err(err)?.json().await.map_err(err)?;
    v["access_token"].as_str().map(String::from).ok_or_else(|| v.to_string())
}

#[tauri::command]
async fn g_pull(id: String, secret: String, rt: String) -> Result<String, String> {
    let t = access(&id, &secret, &rt).await?;
    let c = reqwest::Client::new();
    let list: Value = c.get(FILES).bearer_auth(&t)
        .query(&[("q", format!("name='{}' and trashed=false", NAME)), ("fields", "files(id)".to_string())])
        .send().await.map_err(err)?.json().await.map_err(err)?;
    match list["files"][0]["id"].as_str() {
        Some(fid) => {
            let text = c.get(format!("{}/{}?alt=media", FILES, fid)).bearer_auth(&t)
                .send().await.map_err(err)?.text().await.map_err(err)?;
            Ok(json!({ "id": fid, "text": text }).to_string())
        }
        None => Ok(json!({ "id": null, "text": null }).to_string()),
    }
}

#[tauri::command]
async fn g_push(id: String, secret: String, rt: String, fid: String, text: String) -> Result<String, String> {
    let t = access(&id, &secret, &rt).await?;
    let c = reqwest::Client::new();
    let fid = if fid.is_empty() {
        let v: Value = c.post(FILES).bearer_auth(&t).json(&json!({ "name": NAME }))
            .send().await.map_err(err)?.json().await.map_err(err)?;
        v["id"].as_str().ok_or_else(|| v.to_string())?.to_string()
    } else { fid };
    c.patch(format!("https://www.googleapis.com/upload/drive/v3/files/{}?uploadType=media", fid))
        .bearer_auth(&t).header("Content-Type", "application/json").body(text)
        .send().await.map_err(err)?.error_for_status().map_err(err)?;
    Ok(fid)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![read_file, write_file, http_post, http_get, g_start, g_poll, g_pull, g_push])
        .run(tauri::generate_context!())
        .expect("error while running app");
}
