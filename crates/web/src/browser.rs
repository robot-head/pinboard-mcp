use wasm_bindgen::prelude::*;
#[wasm_bindgen(module = "/public/bridge.js")]
extern "C" {
    #[wasm_bindgen(js_name=stopLive)]
    pub fn stop_live();
    #[wasm_bindgen(js_name=startDialog)]
    pub fn start_dialog();
    #[wasm_bindgen(js_name=endDialog)]
    pub fn end_dialog();
    #[wasm_bindgen(js_name=observeCanvas)]
    pub fn observe_canvas(callback: &js_sys::Function);
    #[wasm_bindgen(js_name=openLive)]
    pub fn open_live(id: &str, callback: &js_sys::Function);
    #[wasm_bindgen(js_name=sendCamera)]
    pub fn send_camera(x: f64, y: f64, zoom: f64, name: &str);
    #[wasm_bindgen(js_name=capturePointer)]
    pub fn capture_pointer(id: i32);
    #[wasm_bindgen(js_name=setLocation)]
    pub fn set_location(id: &str);
    #[wasm_bindgen(js_name=copyText)]
    pub fn copy_text(text: &str) -> js_sys::Promise;
    #[wasm_bindgen(js_name=focusElement)]
    pub fn focus_element(id: &str);
    #[wasm_bindgen(js_name=toggleVoice)]
    pub fn toggle_voice(callback: &js_sys::Function, initial: &str);
    #[wasm_bindgen(js_name=stopVoice)]
    pub fn stop_voice();
    #[wasm_bindgen(js_name=exportPng)]
    pub fn export_png(json: &str) -> js_sys::Promise;
}
