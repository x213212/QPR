use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::io;
use warp::Filter;
use warp::Reply; // 添加此导入
use dotenv::dotenv;
use std::env;
use std::collections::HashMap;
use futures::future::join_all;

use std::sync::Arc;
use tokio::sync::RwLock;

// ===========================
// 可配置的常數
// ===========================

// 伺服器埠號設定
const SERVER_PORT: u16 = 3030;

// 程式碼檔案的副檔名清單
const CODE_FILE_EXTENSIONS: &[&str] = &[
    "rs", "py", "js", "ts", "java", "cpp", "c", "go", "sh", "rb", "bat", "cs", "resx","h","md",
];

const FILE_SUMMARY_PROMPT: &str = "SYSTEM:你是一個專業的軟體分析工程師，給你程式碼你可以描述原始碼的大致實現那些具體功能，並精確地請以 「繁體中文 」的方式撰寫，每個大概寫個50個字。\nUSER:{}\nASSISTANT";
const FILE_SUMMARY_PROMPT2: &str = "SYSTEM:你是一個專業的軟體分析工程師，給你程式碼你可以描述原始碼的大致實現那些具體功能，並精確地請以 「繁體中文 」的方式撰寫，你正在總結片段大概寫個150個字。\nUSER:{}\nASSISTANT";
// 專案目錄路徑設定
const PROJECT_PATH: &str = "/root/angr_ctf";

const FOLDER_ANALYSIS_PROMPT: &str = 
    "SYSTEM:Please analyze the following folder names and filter out those that are likely to be user-written source code directories. If no directories are found, please use the default path: /root/c. The result should only return a JSON structure in the following format: {\"analysis_key\": [folder names that meet the criteria]}, where 'analysis_key' is the only key, and the corresponding value is an array of folder names that meet the criteria. Please ensure that the returned JSON structure contains only this key-value pair and does not include any additional information or explanations.\nThe list of folder names is as follows:\n{folders}\n{extra_folders}";

// ===========================
// llama 請求和回應結構
// ===========================
#[derive(Serialize, Deserialize)]
struct LlamaRequest {
    prompt: String,
    n_predict: usize,
    temperature: f32,
    top_k: usize,
    top_p: f32,
}

// ===========================
// GPT 請求和回應結構
// ===========================

#[derive(Serialize, Deserialize)]
struct GPTRequest {
    model: String,
    messages: Vec<Message>,
}

#[derive(Serialize, Deserialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct GPTResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: Message,
}

// 定義用於解析 GPT 分析回應的結構
#[derive(Serialize, Deserialize)]
struct GPTAnalysis {
    analysis_key: Vec<String>,
}

// 過濾隱藏目錄與不重要的目錄
fn is_hidden_or_common_ignore(path: &Path) -> bool {
    let hidden_dirs = vec![".git", ".github", ".pytest_cache", ".gitignore", "site-packages"];
    if let Some(dir_name) = path.file_name() {
        if let Some(dir_name_str) = dir_name.to_str() {
            return hidden_dirs.contains(&dir_name_str);
        }
    }
    false
}
use serde_json::Value; // 引入通用的 Value 類型
// 使用 Llama 過濾檔案並生成摘要
async fn summarize_file_with_llama(
    file_content: String,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let client = Client::new();
    let max_lines = 500; // 設定每次請求的最大行數
    let mut summaries = Vec::new();

    // 將 file_content 切割成多個片段
    let lines: Vec<&str> = file_content.lines().collect(); // 將內容切割為行
    let mut start = 0;

    while start < lines.len() {
        let end = std::cmp::min(start + max_lines, lines.len());
        let chunk = lines[start..end].join("\n"); // 合併行為一個片段

        // 替換 FILE_SUMMARY_PROMPT 中的佔位符
        let prompt = FILE_SUMMARY_PROMPT.replace("{}", &chunk);

        // 設置 POST 請求的 body
        let request_body = serde_json::json!( {
            "n_predict": 4096,
            "temperature": 0.2,
            "stop": ["</s>", "<|end|>", "<|eot_id|>", "<|end_of_text|>", "<|im_end|>", "<|EOT|>", "<|END_OF_TURN_TOKEN|>", "<|end_of_turn|>", "<|endoftext|>", "ASSISTANT", "USER"],
            "repeat_last_n": 0,
            "repeat_penalty": 0.80,
            "penalize_nl": false,
            "top_k": 40,
            "top_p": 0.79,
            "min_p": 0.43,
            "tfs_z": 1,
            "typical_p": 1,
            "presence_penalty": 0,
            "frequency_penalty": 0,
            "mirostat": 0,
            "mirostat_tau": 5,
            "mirostat_eta": 0.1,
            "grammar": "",
            "n_probs": 0,
            "min_keep": 0,
            "prompt": prompt.trim()
        });

        // 發送請求
        let res = client
            .post("http://127.0.0.1:9090/completion")
            .json(&request_body)
            .send()
            .await?;

        let res_text = res.text().await?;
        let res_json: Value = serde_json::from_str(&res_text)?;

        // 檢查 JSON 回應中是否存在 "content" 欄位
        if let Some(summary) = res_json.get("content") {
            if let Some(summary_str) = summary.as_str() {
                summaries.push(summary_str.to_string()); // 將摘要添加到總結中
            }
        }

        start += max_lines; // 移動到下一個片段
    }

    // 合併所有摘要為一個大段落
    let final_summary = summaries.join(" "); // 使用空格合併片段

    // 最終的摘要調用
    let final_prompt = FILE_SUMMARY_PROMPT2.replace("{}", &final_summary);
    let final_request_body = serde_json::json!( {
        "n_predict": 4096,
        "temperature": 0.28,
        "stop": ["</s>", "<|end|>", "<|eot_id|>", "<|end_of_text|>", "<|im_end|>", "<|EOT|>", "<|END_OF_TURN_TOKEN|>", "<|end_of_turn|>", "<|endoftext|>", "ASSISTANT", "USER"],
        "repeat_last_n": 0,
        "repeat_penalty": 0.80,
        "penalize_nl": false,
        "top_k": 40,
        "top_p": 0.79,
        "min_p": 0.43,
        "tfs_z": 1,
        "typical_p": 1,
        "presence_penalty": 0,
        "frequency_penalty": 0,
        "mirostat": 0,
        "mirostat_tau": 5,
        "mirostat_eta": 0.1,
        "grammar": "",
        "n_probs": 0,
        "min_keep": 0,
        "prompt": final_prompt.trim()
    });

    // 發送最終請求
    let res = client
        .post("http://127.0.0.1:9090/completion")
        .json(&final_request_body)
        .send()
        .await?;

    let res_text = res.text().await?;
    let res_json: Value = serde_json::from_str(&res_text)?;

    // 檢查 JSON 回應中是否存在 "content" 欄位
    if let Some(final_summary_content) = res_json.get("content") {
        if let Some(final_summary_str) = final_summary_content.as_str() {
            return Ok(final_summary_str.to_string());
        }
    }

    Err("無法從 Llama 回應中提取最終摘要".into())
}


use regex::Regex;
use std::error::Error;
// 使用 Llama 過濾資料夾
async fn analyze_folders_with_llama(
    folders: &str,
    extra_folders: &str,
) -> Result<String, Box<dyn Error>> {
    let client = Client::new();

    // 替換 prompt 中的資料夾內容
    let prompt = format!(
        "SYSTEM:Please analyze the following folder names and filter out those that are likely to be user-written source code directories. If no directories are found, please use the default path: /root/c. The result should only return a JSON structure in the following format: {{\"analysis_key\": [folder names that meet the criteria]}}, where 'analysis_key' is the only key, and the corresponding value is an array of folder names that meet the criteria. Please ensure that the returned JSON structure contains only this key-value pair and does not include any additional information or explanations.\nThe list of folder names is as follows\n\n\nUSER:{}{}\nASSISTANT",
        folders.trim(), // 清除前後空白
        extra_folders.trim() // 清除前後空白
    );
    // println!("伺服器回應: {}", prompt);
    // 構建 Llama 請求
    let request_body = serde_json::json!( {
        "n_predict": 4096,
        "temperature": 0.28,
        "stop": ["</s>", "<|end|>", "<|eot_id|>", "<|end_of_text|>", "<|im_end|>", "<|EOT|>", "<|END_OF_TURN_TOKEN|>", "<|end_of_turn|>", "<|endoftext|>", "ASSISTANT", "USER"],
        "repeat_last_n": 0,
        "repeat_penalty": 0.84,
        "penalize_nl": false,
        "top_k": 31,
        "top_p": 0.79,
        "min_p": 0.43,
        "tfs_z": 1,
        "typical_p": 1,
        "presence_penalty": 0,
        "frequency_penalty": 0,
        "mirostat": 0,
        "mirostat_tau": 5,
        "mirostat_eta": 0.1,
        "grammar": "",
        "n_probs": 0,
        "min_keep": 0,

        "prompt": prompt
    });

    // 發送請求到 Llama 伺服器
    let res = client
        .post("http://127.0.0.1:9090/completion")
        .json(&request_body)
        .send()
        .await?;

    let res_text = res.text().await?;

    // 打印伺服器回應內容，方便調試
    println!("伺服器回應: {}", res_text);

    // 嘗試解析伺服器回應為 JSON
    let res_json: serde_json::Value = serde_json::from_str(&res_text)?;

    // 提取 content 欄位
    if let Some(content) = res_json.get("content") {
        if let Some(content_str) = content.as_str() {
            // 使用正則表達式匹配 JSON 結構，尋找最後出現的 { ... } 包含 "analysis_key" 的結構
            let json_re = Regex::new(r#"\{[^{}]*"analysis_key":[^{}]*\}"#)?;

            // 嘗試匹配
            if let Some(captures) = json_re.captures(content_str) {
                let json_str = captures.get(0).map_or("", |m| m.as_str());

                // 顯示提取到的 JSON 結構
                println!("提取到的 JSON 結構: {}", json_str);
                return Ok(json_str.to_string());
            }
        }
    }
   
    // 如果解析失敗，返回錯誤
    Err("無法從 Llama 回應中提取 JSON 結構".into())
}


// 定義檔案資訊結構
#[derive(Debug, Serialize, Deserialize, Clone)]
struct FileInfo {
    name: String,
    summary: Option<String>,
}

// 定義目錄結構
#[derive(Debug, Serialize, Deserialize, Clone)]
struct Directory {
    name: String,
    subdirs: Vec<Directory>,
    files: Vec<FileInfo>,
    path: String,
}

impl Directory {
    fn new(name: String, path: String) -> Self {
        Directory {
            name,
            subdirs: Vec::new(),
            files: Vec::new(),
            path,
        }
    }

    // 修改後的 from_path 函數，添加了排序功能
    fn from_path(path: &Path, collect_files: bool) -> Self {
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_str()
            .unwrap_or("")
            .to_string();
        let path_str = path.to_string_lossy().to_string();

        let mut dir = Directory::new(name.clone(), path_str.clone());

        if let Ok(entries) = fs::read_dir(path) {
            let mut dirs = Vec::new();
            let mut files = Vec::new();
            for entry in entries.flatten() {
                let entry_path = entry.path();
                if entry_path.is_dir() && !is_hidden_or_common_ignore(&entry_path) {
                    dirs.push(entry_path);
                } else if collect_files && entry_path.is_file() && Directory::is_code_file(&entry_path) {
                    files.push(entry_path);
                }
            }

            // 對目錄和檔案進行排序
            dirs.sort_by(|a, b| a.file_name().unwrap_or_default().cmp(&b.file_name().unwrap_or_default()));
            files.sort_by(|a, b| a.file_name().unwrap_or_default().cmp(&b.file_name().unwrap_or_default()));

            for entry_path in dirs {
                dir.subdirs.push(Directory::from_path(&entry_path, collect_files));
            }

            for entry_path in files {
                if let Some(file_name) = entry_path.file_name() {
                    if let Some(file_name_str) = file_name.to_str() {
                        dir.files.push(FileInfo {
                            name: file_name_str.to_string(),
                            summary: None,
                        });
                    }
                }
            }
        }

        dir
    }

    // 判斷檔案是否為程式碼檔案
    fn is_code_file(path: &Path) -> bool {
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            CODE_FILE_EXTENSIONS.contains(&ext)
        } else {
            false
        }
    }
// 收集所有資料夾名稱，格式化為字串，並標示每個資料夾的上層（供 GPT/Llama 使用）
fn collect_folders(&self) -> String {
    let mut result = String::new();
    result.push_str("");
    self.collect_folders_recursively(0, &mut result, None, false);
    result
}

// 修改後的遞迴收集函數，新增 parent_name 用來追蹤上層資料夾，並顯示上層結構
fn collect_folders_recursively(
    &self,
    depth: usize,
    result: &mut String,
    parent_name: Option<&str>, // 追蹤上層資料夾
    include_files: bool,
) {
    // 使用縮排來表示資料夾層次結構
    for _ in 0..depth {
        result.push_str("  "); // 每個層級增加兩個空格
    }

    // 標記為資料夾並添加名稱和上層資料夾
    if let Some(parent) = parent_name {
        result.push_str(&format!("folder: {} (top folder: {})\n", self.name, parent));
    } else {
        result.push_str(&format!("folder: {} (root folder)\n", self.name)); // 如果沒有上層，標示為根資料夾
    }

    // 遞迴列出子資料夾，並將當前資料夾設為子資料夾的上層
    for subdir in &self.subdirs {
        subdir.collect_folders_recursively(depth + 1, result, Some(&self.name), include_files);
    }

    // 如果需要，列出檔案
    if include_files {
        for file in &self.files {
            for _ in 0..(depth + 1) {
                result.push_str("  "); // 顯示檔案的縮排，略多於資料夾
            }
            // 標記為檔案並添加上層資料夾名稱
            result.push_str(&format!("檔案: {} (上層: {})\n", file.name, self.name));
        }
    }
}

    // 收集需要生成摘要的檔案
    fn collect_files_to_summarize(&mut self, filtered_folders: &[String]) -> Vec<(String, String)> {
        let mut files = Vec::new();
        if filtered_folders.iter().any(|folder| self.name.to_lowercase() == folder.to_lowercase()) {
            // 重新從檔案系統中收集其所有子目錄和檔案
            *self = Directory::from_path(Path::new(&self.path), true);

            // 收集當前目錄及其子目錄的所有檔案
            self.collect_all_files(&mut files);
        } else {
            // 遞迴檢查子目錄
            for subdir in &mut self.subdirs {
                files.extend(subdir.collect_files_to_summarize(filtered_folders));
            }
        }
        files
    }

    // 收集當前目錄及其所有子目錄的所有檔案
    fn collect_all_files(&self, files: &mut Vec<(String, String)>) {
        for file in &self.files {
            let file_path = Path::new(&self.path).join(&file.name).to_string_lossy().to_string();
            files.push((file_path, file.name.clone()));
        }
        for subdir in &self.subdirs {
            subdir.collect_all_files(files);
        }
    }

    // 更新檔案摘要
    fn update_file_summary(&mut self, file_path: &str, summary: String) {
        if self.path == file_path {
            // 當前路徑即為檔案路徑
            if let Some(file) = self.files.iter_mut().find(|f| {
                let full_path = format!("{}/{}", self.path, f.name);
                full_path == file_path
            }) {
                file.summary = Some(summary);
            }
            return;
        }

        // 遞迴更新子目錄
        for subdir in &mut self.subdirs {
            if file_path.starts_with(&subdir.path) {
                subdir.update_file_summary(file_path, summary.clone());
            }
        }
    }
}

// 從使用者輸入取得要保留的資料夾名稱
fn get_folders_to_add() -> String {
    println!("請輸入要保留的資料夾名稱（以逗號分隔，或輸入 'ok' 表示完成）：");
    let mut input = String::new();
    io::stdin().read_line(&mut input).expect("無法讀取輸入");
    input.trim().to_string()
}

// 定義進度結構
#[derive(Debug, Serialize, Clone)]
struct Progress {
    total_files: usize,
    completed_files: usize,
    summaries: HashMap<String, String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 使用有效的 API 金鑰
    dotenv().ok();
    let api_key = env::var("OPENAI_API_KEY").expect("未設置 OPENAI_API_KEY");

    // 指定專案目錄路徑
    let path = Path::new(PROJECT_PATH);
    let mut project = Directory::from_path(path, false); // 初次僅收集目錄

    // 1. 初始收集資料夾
    let folders = project.collect_folders();
    println!("收集的資料夾：\n{}", folders);

    // 2. 初始呼叫 GPT 進行資料夾過濾
    let mut extra_prompt = String::new(); // 保存使用者補充的資料夾
    
    let filtered_folders = analyze_folders_with_llama(&folders.to_string(), &extra_prompt.to_string()).await?;

    println!("重新過濾後的結果：\n{}", filtered_folders);

    // 3. 解析 GPT 回應
    let analysis: GPTAnalysis = serde_json::from_str(&filtered_folders)?;
    let mut filtered_folder_list = analysis.analysis_key;

    // 4. 互動式資料夾選擇
    loop {
        let folders_to_add = get_folders_to_add();
        if folders_to_add.to_lowercase() == "ok" {
            break;
        }

        // 將新增資料夾加到 GPT 請求中
        extra_prompt.push_str(&format!(", 請再額外判斷 {}", folders_to_add));

        // 再次過濾資料夾，包含新的資料夾清單
        let updated_folders = project.collect_folders();
        let filtered_folders = analyze_folders_with_llama(&updated_folders.to_string(), &extra_prompt.to_string()).await?;


        println!("重新過濾後的結果：\n{}", filtered_folders);

        // 解析更新後的 GPT 回應
        let analysis: GPTAnalysis = serde_json::from_str(&filtered_folders)?;
        filtered_folder_list = analysis.analysis_key.clone();
    }

    // 5. 列出最終選定的資料夾結構
    println!("最終選定的資料夾為：\n{:#?}", filtered_folder_list);

    // 6. 為選定的資料夾收集檔案並生成摘要
    let files_to_summarize = project.collect_files_to_summarize(&filtered_folder_list);

    // 定義進度狀態
    let progress = Arc::new(RwLock::new(Progress {
        total_files: files_to_summarize.len(),
        completed_files: 0,
        summaries: HashMap::new(),
    }));

    // 共享的項目目錄結構
    let project_arc = Arc::new(RwLock::new(project));

    // 異步生成檔案摘要
    let mut tasks = Vec::new();
    for (file_path, _file_name) in files_to_summarize {
        let api_key_clone = api_key.clone();
        let progress_clone = Arc::clone(&progress);
        let project_clone = Arc::clone(&project_arc);
        tasks.push(tokio::spawn(async move {
            let file_content = fs::read_to_string(&file_path).unwrap_or_default();
            
            let summary = if file_content.trim().is_empty() {
                "檔案內容為空".to_string()
            } else {
                summarize_file_with_llama(file_content.clone())
                .await
                .unwrap_or_else(|_| "摘要生成失敗".to_string())
            
            };

            // 更新進度
            {
                let mut progress = progress_clone.write().await;
                progress.completed_files += 1;
                progress.summaries.insert(file_path.clone(), summary.clone());
            }

            // 更新項目目錄結構中的摘要
            {
                let mut project = project_clone.write().await;
                project.update_file_summary(&file_path, summary);
            }

            println!("已完成摘要：{}", file_path);
        }));
    }

    // 等待所有任務完成
    join_all(tasks).await;

    // 從 Arc 中取出項目目錄結構
    let project = Arc::try_unwrap(project_arc).unwrap().into_inner();

    // 7. 準備啟動 Web 伺服器顯示Quick Project Report 和進度
    let project_arc = Arc::new(RwLock::new(project));
    let progress_arc = Arc::clone(&progress);

    // 定義 /filtered-tree 端點
    let project_clone = Arc::clone(&project_arc);

    let filtered_tree_route = warp::path("filtered-tree")
        .and(warp::get())
        .and_then({
            let project_clone = Arc::clone(&project_clone);
            move || {
                let project_clone = Arc::clone(&project_clone);
                async move {
                    let project = project_clone.read().await;
                    Ok::<_, std::convert::Infallible>(warp::reply::json(&*project))
                }
            }
        });

    // 定義 /progress 端點
    let progress_route = warp::path("progress")
        .and(warp::get())
        .and_then({
            let progress_arc = Arc::clone(&progress_arc);
            move || {
                let progress_arc = Arc::clone(&progress_arc);
                async move {
                    let progress = progress_arc.read().await;
                    Ok::<_, std::convert::Infallible>(warp::reply::json(&*progress))
                }
            }
        });
        let index_html = warp::path::end().map(|| {
            warp::reply::html(
                r#"
                <!DOCTYPE html>
                <html lang="zh-TW">
                <head>
                    <meta charset="UTF-8">
                    <title>Quick Project Report</title>
                    <link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/jstree/dist/themes/default/style.min.css" />
                    <link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/prism/1.28.0/themes/prism-okaidia.min.css">
                    <style>
                        body {
                            font-family: Arial, sans-serif;
                            margin: 0;
                            padding: 20px;
                            background-color: #1e1e1e; /* 黑色背景 */
                            color: #d4d4d4; /* 淡灰色文字 */
                            display: flex;
                            flex-direction: column;
                            height: 100vh;
                        }
                        h1, h2 {
                            text-align: center;
                            color: #d4d4d4;
                        }
                        #controls {
                            text-align: center;
                            margin-bottom: 20px;
                        }
                        button {
                            margin: 0 10px;
                            padding: 10px 20px;
                            font-size: 16px;
                            background-color: #007acc;
                            color: #fff;
                            border: none;
                            cursor: pointer;
                        }
                        button:hover {
                            background-color: #005f99;
                        }
                        #main {
                            display: flex;
                            flex: 1;
                        }
                        #jstree {
                            width: 30%;
                            background-color: #252526; /* 深灰色背景 */
                            padding: 10px;
                            overflow-y: auto;
                            color: #d4d4d4;
                        }
                        #summary {
                            width: 70%;
                            padding: 20px;
                            background-color: #1e1e1e;
                            margin-left: 20px;
                            overflow-y: auto;
                            color: #d4d4d4;
                        }
                        pre {
                            background-color: #1e1e1e;
                            padding: 10px;
                            overflow-x: auto;
                            white-space: pre-wrap;
                            word-wrap: break-word;
                        }
                        code {
                            font-family: Consolas, 'Courier New', monospace;
                        }
                        /* Tabs Style */
                        .tab-container {
                            width: 100%;
                            display: flex;
                            justify-content: center;
                            margin-bottom: 20px;
                        }
                        .tab {
                            padding: 10px 20px;
                            cursor: pointer;
                            background-color: #007acc;
                            color: white;
                            margin: 0 5px;
                            border: none;
                        }
                        .tab.active {
                            background-color: #005f99;
                        }
                        .content-container {
                            display: none;
                        }
                        .content-container.active {
                            display: block;
                        }
                    </style>
                    <script src="https://cdn.jsdelivr.net/npm/jquery@3.6.0/dist/jquery.min.js"></script>
                    <script src="https://cdn.jsdelivr.net/npm/jstree@3.3.12/dist/jstree.min.js"></script>
                    <script src="https://cdn.jsdelivr.net/npm/prismjs@1.28.0/prism.min.js"></script>
                </head>
                <body>
                    <h1>Quick Project Report </h1>
        
                    <!-- Tabs -->
                    <div class="tab-container">
                        <button class="tab active" onclick="showTab('file-tab')">檔案目錄與程式碼</button>
                        <button class="tab" onclick="showTab('summary-tab')">總摘要</button>
                    </div>
        
                    <!-- Content: File Directory and Code -->
                    <div id="file-tab" class="content-container active">
                        <div id="controls">
                            <button onclick="fetchTree()">顯示目錄樹</button>
                            <button onclick="fetchProgress()">查看摘要進度</button>
                        </div>
                        <div id="main">
                            <div id="jstree"></div>
                            <div id="summary">
                                <h2>檔案摘要和程式碼</h2>
                                <div id="file-summary">請選擇一個檔案以查看摘要和程式碼。</div>
                            </div>
                        </div>
                    </div>
        
                    <!-- Content: Total Summary -->
                    <div id="summary-tab" class="content-container">
                        <h2>總摘要</h2>
                        <div id="progress"></div>
                    </div>
        
                    <script>
                        let progressData = null;
        
                        function showTab(tabId) {
                            // Hide all content containers
                            document.querySelectorAll('.content-container').forEach(tab => {
                                tab.classList.remove('active');
                            });
        
                            // Remove 'active' class from all tabs
                            document.querySelectorAll('.tab').forEach(tab => {
                                tab.classList.remove('active');
                            });
        
                            // Show the selected tab and activate the corresponding button
                            document.getElementById(tabId).classList.add('active');
                            document.querySelector(`[onclick="showTab('${tabId}')"]`).classList.add('active');
                        }
        
                        async function fetchTree() {
                            try {
                                const response = await fetch('/filtered-tree');
                                const data = await response.json();
                                displayTree(data);
                            } catch (error) {
                                console.error('抓取目錄樹時出錯:', error);
                            }
                        }
        
                        async function fetchProgress() {
                            try {
                                const response = await fetch('/progress');
                                const data = await response.json();
                                progressData = data;
                                displayProgress(data, document.getElementById('progress'));
                            } catch (error) {
                                console.error('抓取進度時出錯:', error);
                            }
                        }
        
                        function displayProgress(progress, parentElement) {
                            parentElement.innerHTML = '';
                            const progressText = `已完成 ${progress.completed_files} / ${progress.total_files} 個摘要`;
                            const progressDiv = document.createElement('div');
                            progressDiv.innerText = progressText;
                            parentElement.appendChild(progressDiv);
        
                            const summariesUl = document.createElement('ul');
                            for (const [filePath, summary] of Object.entries(progress.summaries)) {
                                const li = document.createElement('li');
                                li.textContent = `${filePath}: ${summary}`;
                                summariesUl.appendChild(li);
                            }
                            parentElement.appendChild(summariesUl);
                        }
        
                        function displayTree(directory) {
                            const treeData = [convertToJsTreeFormat(directory)];
        
                            $('#jstree').jstree('destroy'); // 重置 jstree
                            $('#jstree').jstree({
                                'core': {
                                    'data': treeData,
                                    'themes': {
                                        'variant': 'large',
                                        'dots': true,
                                        'icons': true
                                    }
                                },
                                'plugins': ['wholerow']
                            });
        
                            // 綁定節點點擊事件
                            $('#jstree').on('select_node.jstree', function (e, data) {
                                const node = data.node;
                                if (node.original && node.original.type === 'file') {
                                    const filePath = node.original.path;
                                    displayFileSummaryAndCode(filePath);
                                    showTab('file-tab');  // 點擊檔案後顯示檔案目錄和程式碼頁
                                } else {
                                    $('#file-summary').html('請選擇一個檔案以查看摘要和程式碼。');
                                }
                            });
                        }
        
                        function convertToJsTreeFormat(directory) {
                            const node = {
                                text: directory.name,
                                children: [],
                                state: {
                                    opened: true
                                },
                                type: 'folder',
                                path: directory.path
                            };
        
                            directory.files.sort((a, b) => a.name.localeCompare(b.name));
                            for (const file of directory.files) {
                                node.children.push({
                                    text: file.name,
                                    type: 'file',
                                    path: `${directory.path}/${file.name}`,
                                    summary: file.summary || '無摘要',
                                    icon: 'jstree-file'
                                });
                            }
        
                            directory.subdirs.sort((a, b) => a.name.localeCompare(b.name));
                            for (const subdir of directory.subdirs) {
                                node.children.push(convertToJsTreeFormat(subdir));
                            }
        
                            return node;
                        }
        
                        async function displayFileSummaryAndCode(filePath) {
                            if (!progressData) {
                                $('#file-summary').html('請先點擊 "查看摘要進度" 以載入摘要資料。');
                                return;
                            }
        
                            const summary = progressData.summaries[filePath];
                            let codeContent = '';
        
                            try {
                                const response = await fetch('/get-file?path=' + encodeURIComponent(filePath));
                                if (response.ok) {
                                    codeContent = await response.text();
                                } else {
                                    codeContent = '無法取得檔案內容。';
                                }
                            } catch (error) {
                                codeContent = '抓取檔案內容時出錯。';
                            }
        
                            const fileExtension = filePath.split('.').pop().toLowerCase();
                            const languageClass = languageMapping[fileExtension] || 'plaintext';
                            const codeHtml = `<pre><code class="language-${languageClass}">${escapeHtml(codeContent)}</code></pre>`;
        
                            Prism.highlightAll();
        
                            if (summary) {
                                $('#file-summary').html(`<h3>摘要：</h3><p>${summary}</p><h3>程式碼：</h3>${codeHtml}`);
                            } else {
                                $('#file-summary').html(`<h3>摘要：</h3><p>此檔案沒有摘要。</p><h3>程式碼：</h3>${codeHtml}`);
                            }
                        }
        
                        function escapeHtml(text) {
                            return text
                                .replace(/&/g, '&amp;')
                                .replace(/</g, '&lt;')
                                .replace(/>/g, '&gt;')
                                .replace(/"/g, '&quot;')
                                .replace(/'/g, '&#039;');
                        }
        
                        let languageMapping = {
                            "rs": "rust",
                            "py": "python",
                            "js": "javascript",
                            "ts": "typescript",
                            "java": "java",
                            "cpp": "cpp",
                            "c": "c",
                            "go": "go",
                            "sh": "bash",
                            "rb": "ruby",
                            "bat": "batch",
                            "cs": "csharp",
                            "resx": "xml",
                            "h": "clike",
                            "md": "markdown"
                        };
                    </script>
                </body>
                </html>
                "#
            )
        });
        
        

    // 添加新的路由來處理檔案內容請求
    let get_file_route = warp::path("get-file")
        .and(warp::get())
        .and(warp::query::<HashMap<String, String>>())
        .and_then({
            move |params: HashMap<String, String>| async move {
                let response = if let Some(path) = params.get("path") {
                    if let Ok(content) = fs::read_to_string(path) {
                        warp::reply::html(content).into_response()
                    } else {
                        warp::reply::with_status(
                            warp::reply::html("無法取得檔案內容。"),
                            warp::http::StatusCode::NOT_FOUND,
                        )
                        .into_response()
                    }
                } else {
                    warp::reply::with_status(
                        warp::reply::html("無法取得檔案內容。"),
                        warp::http::StatusCode::NOT_FOUND,
                    )
                    .into_response()
                };
                Ok::<_, std::convert::Infallible>(response)
            }
        });

    // 合併所有路由
    let routes = filtered_tree_route
        .or(progress_route)
        .or(get_file_route)
        .or(index_html);

    // 啟動伺服器
    println!("啟動網頁伺服器，請訪問 http://127.0.0.1:{}", SERVER_PORT);
    warp::serve(routes)
        .run(([127, 0, 0, 1], SERVER_PORT))
        .await;

    Ok(())
}