#[path = "src/schema.rs"]
mod schema;

use std::{
    env, fs,
    path::{Path, PathBuf},
};

use schema::{ProductConfig, ProductFeatures};

fn main() {
    println!("cargo:rerun-if-env-changed=OPPA_PRODUCT_DIR");

    if let Err(error) = build_product() {
        panic!("invalid OPPA product configuration: {error}");
    }
}

fn build_product() -> Result<(), String> {
    let manifest_dir = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR")
            .ok_or_else(|| "Cargo did not set CARGO_MANIFEST_DIR".to_owned())?,
    );
    let configured_dir = env::var_os("OPPA_PRODUCT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| manifest_dir.join("../../products/default"));
    let product_dir = configured_dir
        .canonicalize()
        .map_err(|error| format!("cannot open {}: {error}", configured_dir.display()))?;
    let product_json = product_dir.join("product.json");
    println!("cargo:rerun-if-changed={}", product_json.display());

    let source = fs::read_to_string(&product_json)
        .map_err(|error| format!("cannot read {}: {error}", product_json.display()))?;
    let product: ProductConfig = serde_json::from_str(&source).map_err(|error| {
        format!(
            "{} is not valid schema v1 JSON: {error}",
            product_json.display()
        )
    })?;
    let mut issues = product.validation_issues();
    validate_compiled_features(product.features, &mut issues);
    if !issues.is_empty() {
        return Err(issues.join("\n- "));
    }

    let scheme = product.deep_link_scheme.as_deref().unwrap_or("oppa");
    let assets = collect_assets(&product_dir.join("assets"))?;
    let output_dir = PathBuf::from(
        env::var_os("OUT_DIR").ok_or_else(|| "Cargo did not set OUT_DIR".to_owned())?,
    );
    let generated = generate_source(&source, &assets, scheme);
    fs::write(output_dir.join("embedded_product.rs"), generated)
        .map_err(|error| format!("cannot write generated product source: {error}"))
}

fn validate_compiled_features(features: ProductFeatures, issues: &mut Vec<String>) {
    let checks = [
        (
            features.virtual_printer,
            "CARGO_FEATURE_VIRTUAL_PRINTER",
            "virtualPrinter",
            "virtual-printer",
        ),
        (
            features.system_printer_discovery,
            "CARGO_FEATURE_SYSTEM_PRINTER_DISCOVERY",
            "systemPrinterDiscovery",
            "system-printer-discovery",
        ),
        (
            features.network_printer_discovery,
            "CARGO_FEATURE_NETWORK_PRINTER_DISCOVERY",
            "networkPrinterDiscovery",
            "network-printer-discovery",
        ),
        (
            features.usb_printer_discovery,
            "CARGO_FEATURE_USB_PRINTER_DISCOVERY",
            "usbPrinterDiscovery",
            "usb-printer-discovery",
        ),
        (
            features.remote_diagnostics,
            "CARGO_FEATURE_REMOTE_DIAGNOSTICS",
            "remoteDiagnostics",
            "remote-diagnostics",
        ),
        (
            features.managed_hardware,
            "CARGO_FEATURE_MANAGED_HARDWARE",
            "managedHardware",
            "managed-hardware",
        ),
    ];
    for (enabled, environment, product_name, cargo_name) in checks {
        if enabled && env::var_os(environment).is_none() {
            issues.push(format!(
                "features.{product_name} is true, but Cargo feature `{cargo_name}` is not compiled in"
            ));
        }
    }
}

fn collect_assets(asset_dir: &Path) -> Result<Vec<(String, PathBuf)>, String> {
    println!("cargo:rerun-if-changed={}", asset_dir.display());
    if !asset_dir.exists() {
        return Ok(Vec::new());
    }
    let mut assets = Vec::new();
    visit_assets(asset_dir, asset_dir, &mut assets)?;
    assets.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(assets)
}

fn visit_assets(
    root: &Path,
    current: &Path,
    assets: &mut Vec<(String, PathBuf)>,
) -> Result<(), String> {
    let entries = fs::read_dir(current)
        .map_err(|error| format!("cannot read asset directory {}: {error}", current.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("cannot inspect product asset: {error}"))?;
        let file_type = entry
            .file_type()
            .map_err(|error| format!("cannot inspect {}: {error}", entry.path().display()))?;
        if file_type.is_symlink() {
            return Err(format!(
                "product assets must not be symbolic links: {}",
                entry.path().display()
            ));
        }
        if file_type.is_dir() {
            visit_assets(root, &entry.path(), assets)?;
        } else if file_type.is_file() {
            let relative = entry
                .path()
                .strip_prefix(root)
                .map_err(|_| "asset escaped its configured root".to_owned())?
                .to_string_lossy()
                .replace('\\', "/");
            println!("cargo:rerun-if-changed={}", entry.path().display());
            assets.push((
                relative,
                entry
                    .path()
                    .canonicalize()
                    .map_err(|error| error.to_string())?,
            ));
        }
    }
    Ok(())
}

fn generate_source(source: &str, assets: &[(String, PathBuf)], scheme: &str) -> String {
    let entries = assets
        .iter()
        .map(|(name, path)| {
            format!(
                "    EmbeddedAsset {{ path: {:?}, bytes: include_bytes!({:?}) }},\n",
                name,
                path.to_string_lossy()
            )
        })
        .collect::<String>();
    format!(
        "pub(crate) const EMBEDDED_PRODUCT_JSON: &str = {source:?};\n\
         pub(crate) static EMBEDDED_ASSETS: &[EmbeddedAsset] = &[\n{entries}];\n\
         /// Deep link URL scheme accepted by this binary (e.g. `\"oppa\"` or `\"oppa-dev\"`).\n\
         pub const DEEP_LINK_SCHEME: &str = {scheme:?};\n"
    )
}
