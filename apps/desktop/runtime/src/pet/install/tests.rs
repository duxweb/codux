use super::{html::resolve_custom_pet_install_from_html, package::install_custom_pet_package};
use crate::pet::{PetCustomPetInstallPreview, PetCustomPetInstallRequest};
use std::{
    fs, io,
    io::Write,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};
use url::Url;
use zip::write::SimpleFileOptions;

#[test]
fn resolves_petdex_preview_from_html() {
    let page_url = Url::parse("https://petdex.crafter.run/pets/spark").unwrap();
    let preview = resolve_custom_pet_install_from_html(
        PetCustomPetInstallRequest {
            page_url: page_url.to_string(),
            display_name: String::new(),
        },
        r#"
            <meta property="og:title" content="Spark — Petdex">
            <meta name="description" content="A bright custom pet">
            <script>window.zipUrl = "https://cdn.petdex.crafter.run/spark.zip";</script>
            "#,
        &page_url,
    )
    .unwrap();

    assert_eq!(preview.slug, "spark");
    assert_eq!(preview.display_name, "Spark");
    assert_eq!(preview.description, "A bright custom pet");
    assert_eq!(preview.zip_url, "https://cdn.petdex.crafter.run/spark.zip");
}

#[test]
fn installs_custom_pet_package_from_zip_bytes() {
    let support_dir = temp_dir("pet-install");
    let preview = PetCustomPetInstallPreview {
        page_url: "https://petdex.crafter.run/pets/spark".to_string(),
        zip_url: "https://cdn.petdex.crafter.run/spark.zip".to_string(),
        slug: "spark".to_string(),
        display_name: "Spark".to_string(),
        description: "Preview description".to_string(),
        image_url: None,
        local_image_path: None,
    };
    let zip = pet_package_zip();

    let pet = install_custom_pet_package(&support_dir, preview, &zip).unwrap();

    assert_eq!(pet.id, "spark");
    assert_eq!(pet.display_name, "Spark");
    assert_eq!(pet.description, "Preview description");
    assert!(support_dir.join("custom-pets/spark/pet.json").is_file());
    assert!(support_dir.join("custom-pets/spark/sprite.png").is_file());

    fs::remove_dir_all(support_dir).ok();
}

fn pet_package_zip() -> Vec<u8> {
    let mut cursor = io::Cursor::new(Vec::new());
    {
        let mut writer = zip::ZipWriter::new(&mut cursor);
        let options = SimpleFileOptions::default();
        writer.start_file("spark/pet.json", options).unwrap();
        writer
            .write_all(
                br#"{"id":"spark","displayName":"","description":"","spritesheetPath":"sprite.png"}"#,
            )
            .unwrap();
        writer.start_file("spark/sprite.png", options).unwrap();
        writer.write_all(&[1_u8, 2, 3]).unwrap();
        writer.finish().unwrap();
    }
    cursor.into_inner()
}

fn temp_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("codux-gpui-{label}-{nanos}"))
}
