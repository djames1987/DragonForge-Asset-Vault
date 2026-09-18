use crate::{
    error::{AppError, AppResult},
    models::AssetRow,
    storage::Storage,
};
use image::{ImageBuffer, Rgba};
use std::{
    cmp::{max, min},
    path::{Path, PathBuf},
};

const MAX_PREVIEW_DIMENSION: u32 = 640;

pub fn is_previewable_image(extension: Option<&str>) -> bool {
    matches!(
        extension.map(|v| v.to_ascii_lowercase()).as_deref(),
        Some("jpg") | Some("jpeg") | Some("png") | Some("webp")
    )
}

pub fn is_previewable_model(extension: Option<&str>) -> bool {
    matches!(
        extension.map(|v| v.to_ascii_lowercase()).as_deref(),
        Some("obj")
    )
}

pub fn is_previewable_asset(extension: Option<&str>) -> bool {
    is_previewable_image(extension) || is_previewable_model(extension)
}

pub async fn get_or_create_thumbnail(
    storage: &Storage,
    asset: &AssetRow,
) -> AppResult<Option<PathBuf>> {
    get_or_create_preview(storage, asset).await
}

pub async fn get_or_create_preview(
    storage: &Storage,
    asset: &AssetRow,
) -> AppResult<Option<PathBuf>> {
    if !is_previewable_asset(asset.extension.as_deref()) {
        return Ok(None);
    }

    let destination = storage.thumbnail_path(&asset.sha256);
    if tokio::fs::try_exists(&destination).await? {
        return Ok(Some(destination));
    }

    let source = storage.resolve_relative(&asset.storage_path)?;
    let destination_for_worker = destination.clone();
    let asset_id = asset.id.clone();
    let extension = asset.extension.clone().unwrap_or_default();

    tokio::task::spawn_blocking(move || {
        generate_preview(&source, &destination_for_worker, &extension)
    })
    .await
    .map_err(|err| AppError::Other(anyhow::anyhow!("preview worker failed: {err}")))??;

    tracing::info!(
        asset_id = %asset_id,
        extension = %extension,
        preview = %destination.display(),
        "asset preview generated"
    );

    Ok(Some(destination))
}

fn generate_preview(source: &Path, destination: &Path, extension: &str) -> AppResult<()> {
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)?;
    }

    match extension.to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" | "png" | "webp" => generate_image_preview(source, destination),
        "obj" => generate_obj_preview(source, destination),
        _ => Err(AppError::BadRequest(
            "preview requested for unsupported asset type".to_string(),
        )),
    }
}

fn generate_image_preview(source: &Path, destination: &Path) -> AppResult<()> {
    let image = image::open(source).map_err(|err| {
        AppError::Other(anyhow::anyhow!(
            "failed to decode image for preview: {err}"
        ))
    })?;
    let thumbnail = image.thumbnail(MAX_PREVIEW_DIMENSION, MAX_PREVIEW_DIMENSION);
    thumbnail
        .save_with_format(destination, image::ImageFormat::Png)
        .map_err(|err| {
            AppError::Other(anyhow::anyhow!(
                "failed to save image preview: {err}"
            ))
        })?;
    Ok(())
}

#[derive(Clone, Copy)]
struct ViewVertex {
    x: f32,
    y: f32,
    z: f32,
}

struct Triangle {
    a: ViewVertex,
    b: ViewVertex,
    c: ViewVertex,
    depth: f32,
    shade: u8,
}

fn generate_obj_preview(source: &Path, destination: &Path) -> AppResult<()> {
    let options = tobj::LoadOptions {
        triangulate: true,
        single_index: true,
        ..Default::default()
    };

    let (models, _) = tobj::load_obj(source, &options).map_err(|err| {
        AppError::Other(anyhow::anyhow!("failed to parse OBJ for preview: {err}"))
    })?;

    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut triangles: Vec<[usize; 3]> = Vec::new();

    for model in models {
        let mesh = model.mesh;
        let base = positions.len();

        for vertex in mesh.positions.chunks_exact(3) {
            positions.push([vertex[0], vertex[1], vertex[2]]);
        }

        for face in mesh.indices.chunks_exact(3) {
            triangles.push([
                base + face[0] as usize,
                base + face[1] as usize,
                base + face[2] as usize,
            ]);
        }
    }

    if positions.is_empty() || triangles.is_empty() {
        return Err(AppError::BadRequest(
            "OBJ has no renderable triangle geometry".to_string(),
        ));
    }

    let mut min_bound = [f32::INFINITY; 3];
    let mut max_bound = [f32::NEG_INFINITY; 3];
    for p in &positions {
        for axis in 0..3 {
            min_bound[axis] = min_bound[axis].min(p[axis]);
            max_bound[axis] = max_bound[axis].max(p[axis]);
        }
    }

    let center = [
        (min_bound[0] + max_bound[0]) * 0.5,
        (min_bound[1] + max_bound[1]) * 0.5,
        (min_bound[2] + max_bound[2]) * 0.5,
    ];
    let extent = [
        max_bound[0] - min_bound[0],
        max_bound[1] - min_bound[1],
        max_bound[2] - min_bound[2],
    ];
    let max_extent = extent[0].max(extent[1]).max(extent[2]).max(0.0001);

    let yaw = 35.0_f32.to_radians();
    let pitch = -24.0_f32.to_radians();
    let cy = yaw.cos();
    let sy = yaw.sin();
    let cp = pitch.cos();
    let sp = pitch.sin();

    let transformed = positions
        .iter()
        .map(|p| {
            let x = (p[0] - center[0]) / max_extent;
            let y = (p[1] - center[1]) / max_extent;
            let z = (p[2] - center[2]) / max_extent;

            let x1 = x * cy + z * sy;
            let z1 = -x * sy + z * cy;
            let y2 = y * cp - z1 * sp;
            let z2 = y * sp + z1 * cp;

            ViewVertex {
                x: x1,
                y: y2,
                z: z2,
            }
        })
        .collect::<Vec<_>>();

    let mut render_triangles = Vec::with_capacity(triangles.len());
    for face in triangles {
        let a = transformed[face[0]];
        let b = transformed[face[1]];
        let c = transformed[face[2]];

        let ux = b.x - a.x;
        let uy = b.y - a.y;
        let uz = b.z - a.z;
        let vx = c.x - a.x;
        let vy = c.y - a.y;
        let vz = c.z - a.z;

        let nx = uy * vz - uz * vy;
        let ny = uz * vx - ux * vz;
        let nz = ux * vy - uy * vx;
        let len = (nx * nx + ny * ny + nz * nz).sqrt().max(0.0001);
        let facing = (nz / len).abs();
        let light = ((ny / len) * -0.35 + (nz / len) * 0.65).abs();
        let shade = (105.0 + 120.0 * (0.4 * facing + 0.6 * light)).clamp(85.0, 225.0) as u8;

        render_triangles.push(Triangle {
            a,
            b,
            c,
            depth: (a.z + b.z + c.z) / 3.0,
            shade,
        });
    }

    render_triangles.sort_by(|lhs, rhs| {
        lhs.depth
            .partial_cmp(&rhs.depth)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let width = MAX_PREVIEW_DIMENSION;
    let height = MAX_PREVIEW_DIMENSION;
    let mut canvas = ImageBuffer::from_pixel(width, height, Rgba([244, 245, 247, 255]));
    let scale = width.min(height) as f32 * 0.78;
    let cx = width as f32 * 0.5;
    let cy_screen = height as f32 * 0.52;

    for tri in &render_triangles {
        let pa = project(tri.a, cx, cy_screen, scale);
        let pb = project(tri.b, cx, cy_screen, scale);
        let pc = project(tri.c, cx, cy_screen, scale);

        fill_triangle(
            &mut canvas,
            pa,
            pb,
            pc,
            Rgba([tri.shade, tri.shade, tri.shade.saturating_add(8), 255]),
        );
        draw_line(&mut canvas, pa, pb, Rgba([45, 48, 55, 255]));
        draw_line(&mut canvas, pb, pc, Rgba([45, 48, 55, 255]));
        draw_line(&mut canvas, pc, pa, Rgba([45, 48, 55, 255]));
    }

    canvas
        .save_with_format(destination, image::ImageFormat::Png)
        .map_err(|err| {
            AppError::Other(anyhow::anyhow!(
                "failed to save OBJ preview: {err}"
            ))
        })?;
    Ok(())
}

fn project(v: ViewVertex, cx: f32, cy: f32, scale: f32) -> (i32, i32) {
    (
        (cx + v.x * scale) as i32,
        (cy - v.y * scale) as i32,
    )
}

fn edge(a: (i32, i32), b: (i32, i32), p: (i32, i32)) -> i64 {
    (p.0 - a.0) as i64 * (b.1 - a.1) as i64
        - (p.1 - a.1) as i64 * (b.0 - a.0) as i64
}

fn fill_triangle(
    image: &mut ImageBuffer<Rgba<u8>, Vec<u8>>,
    a: (i32, i32),
    b: (i32, i32),
    c: (i32, i32),
    color: Rgba<u8>,
) {
    let width = image.width() as i32;
    let height = image.height() as i32;

    let min_x = max(0, min(a.0, min(b.0, c.0)));
    let max_x = min(width - 1, max(a.0, max(b.0, c.0)));
    let min_y = max(0, min(a.1, min(b.1, c.1)));
    let max_y = min(height - 1, max(a.1, max(b.1, c.1)));

    let area = edge(a, b, c);
    if area == 0 {
        return;
    }

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let p = (x, y);
            let w0 = edge(b, c, p);
            let w1 = edge(c, a, p);
            let w2 = edge(a, b, p);

            let inside = if area > 0 {
                w0 >= 0 && w1 >= 0 && w2 >= 0
            } else {
                w0 <= 0 && w1 <= 0 && w2 <= 0
            };

            if inside {
                image.put_pixel(x as u32, y as u32, color);
            }
        }
    }
}

fn draw_line(
    image: &mut ImageBuffer<Rgba<u8>, Vec<u8>>,
    start: (i32, i32),
    end: (i32, i32),
    color: Rgba<u8>,
) {
    let mut x0 = start.0;
    let mut y0 = start.1;
    let x1 = end.0;
    let y1 = end.1;

    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;

    loop {
        if x0 >= 0
            && y0 >= 0
            && (x0 as u32) < image.width()
            && (y0 as u32) < image.height()
        {
            image.put_pixel(x0 as u32, y0 as u32, color);
        }

        if x0 == x1 && y0 == y1 {
            break;
        }

        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
}
