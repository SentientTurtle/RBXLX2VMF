importScripts('./rbxlx2vmf_web.js');

function html_log(message) {
    postMessage({message_type: "LOG", message: message});
}
self.html_log = html_log;

function html_log_error(message) {
    postMessage({message_type: "LOG_ERROR", message: message});
}
self.html_log_error = html_log_error

function alert(message) {
    postMessage({message_type: "ALERT", message: message})
}
self.alert = alert;


onmessage = async (e) => {
    if (e.data.message_type === "START") {
        await wasm_bindgen('./rbxlx2vmf_web_bg.wasm');

        wasm_bindgen.convert_map(
            'map.vmf',
            e.data.file,
            e.data.is_texture_output_enabled,
            e.data.use_developer_textures,
            e.data.map_scale,
            e.data.auto_skybox_enabled,
            e.data.skybox_clearance,
            e.data.optimization_enabled,
            e.data.skyname,
            e.data.web_origin
        )
            .then((zip_data) => {
                const blob = new Blob([zip_data], {type: 'application/zip'});
                postMessage({message_type: "COMPLETE", blob: blob});
            })
    }
};
