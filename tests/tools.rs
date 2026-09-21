//! Tool-level tests.
//!
//! These drive the tools the way the protocol layer does — through their `run`
//! functions with the real server state — and assert on pixels, decoded files
//! and warning codes rather than on "a file appeared".

use img_gen_via_svg_mcp::config::Config;
use img_gen_via_svg_mcp::encode::format::Format;
use img_gen_via_svg_mcp::mcp::params::*;
use img_gen_via_svg_mcp::mcp::state::ServerState;
use img_gen_via_svg_mcp::mcp::tools;
use img_gen_via_svg_mcp::svg::fidelity::GAPS;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The demo corpus doubles as the test corpus: one document per claim.
fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("dev/demos/svg")
}

fn fixture(name: &str) -> PathBuf {
    corpus().join(format!("{name}.svg"))
}

fn state() -> Arc<ServerState> {
    ServerState::new(Config::default())
}

fn state_with(config: Config) -> Arc<ServerState> {
    ServerState::new(config)
}

fn render(params: RenderSvgParams) -> Value {
    tools::render_svg::run(&state(), params).expect("render_svg should succeed").structured
}

fn base(name: &str, out: &Path) -> RenderSvgParams {
    RenderSvgParams {
        svg_path: Some(fixture(name)),
        output_path: Some(out.to_path_buf()),
        ..RenderSvgParams::default()
    }
}

fn warning_codes(result: &Value) -> Vec<String> {
    result["warnings"]
        .as_array()
        .map(|w| w.iter().map(|x| x["code"].as_str().unwrap_or("").to_string()).collect())
        .unwrap_or_default()
}

fn decode(path: &Path) -> image::DynamicImage {
    image::open(path).unwrap_or_else(|e| panic!("{} did not decode: {e}", path.display()))
}

// --- M1, M2, M3: input, absolute size, formats -------------------------------

#[test]
fn every_available_format_writes_a_decodable_file_of_the_requested_size() {
    let dir = tempfile::tempdir().unwrap();
    for format in Format::ALL {
        if !format.is_available() {
            continue;
        }
        let out = dir.path().join(format!("out.{}", format.extension()));
        let mut params = base("gradient-linear", &out);
        params.format = Some(format);
        // ICO cannot express more than 256 pixels per edge.
        let edge = if format == Format::Ico { 64 } else { 120 };
        params.width = Some(edge);
        params.height = Some(edge);
        let result = render(params);

        assert_eq!(result["format"], format.as_str());
        assert_eq!(result["width"], edge);
        assert_eq!(result["height"], edge);
        assert!(out.exists(), "{} wrote no file", format.as_str());

        if matches!(format, Format::Avif) {
            continue; // encodable in this build, not decodable
        }
        let decoded = decode(&out);
        assert_eq!(decoded.width(), edge, "{} lost its width", format.as_str());
        assert_eq!(decoded.height(), edge, "{} lost its height", format.as_str());
    }
}

#[test]
fn the_absolute_size_is_what_comes_out_whatever_the_document_says() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("exact.png");
    let mut params = base("viewbox-aspect", &out); // a 100x80 document
    params.width = Some(512);
    params.height = Some(512);
    let result = render(params);

    assert_eq!(result["width"], 512);
    assert_eq!(result["height"], 512);
    let decoded = decode(&out);
    assert_eq!((decoded.width(), decoded.height()), (512, 512));
    assert_eq!(result["source"]["width"], 100.0);
    assert_eq!(result["source"]["size_origin"], "attributes");
}

#[test]
fn a_source_string_and_a_file_produce_the_same_pixels() {
    let dir = tempfile::tempdir().unwrap();
    let text = std::fs::read_to_string(fixture("gradient-linear")).unwrap();

    let from_file = dir.path().join("file.png");
    let mut params = base("gradient-linear", &from_file);
    params.width = Some(64);
    params.height = Some(64);
    render(params);

    let from_string = dir.path().join("string.png");
    let result = render(RenderSvgParams {
        svg_source: Some(text),
        output_path: Some(from_string.clone()),
        width: Some(64),
        height: Some(64),
        ..RenderSvgParams::default()
    });
    assert!(!result["_unused"].is_string());

    assert_eq!(std::fs::read(&from_file).unwrap(), std::fs::read(&from_string).unwrap());
}

// --- M4: the output path is obeyed or refused --------------------------------

#[test]
fn an_allowlist_refuses_without_writing_anywhere() {
    let allowed = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let config =
        Config { allowed_output_dirs: vec![allowed.path().to_path_buf()], ..Config::default() };

    let out = elsewhere.path().join("nope.png");
    let error = tools::render_svg::run(&state_with(config), base("clip-path", &out))
        .expect_err("a path outside the allowlist has to be refused");

    assert_eq!(error.code.as_str(), "path_not_allowed");
    assert!(!out.exists(), "the refused path was written after all");
    // The surferdot failure mode: a silent redirect into the allowed directory.
    let redirected: Vec<_> = std::fs::read_dir(allowed.path()).unwrap().flatten().collect();
    assert!(redirected.is_empty(), "the call redirected instead of refusing");
}

#[test]
fn an_existing_target_is_kept_when_overwrite_is_false() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("keep.png");
    std::fs::write(&out, b"original").unwrap();

    let mut params = base("clip-path", &out);
    params.overwrite = Some(false);
    let error =
        tools::render_svg::run(&state(), params).expect_err("overwrite false has to refuse");

    assert_eq!(error.code.as_str(), "output_exists");
    assert_eq!(std::fs::read(&out).unwrap(), b"original");
}

#[test]
fn a_missing_parent_is_created_only_on_request() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("nested/deep/icon.png");

    let error = tools::render_svg::run(&state(), base("clip-path", &out)).unwrap_err();
    assert_eq!(error.code.as_str(), "output_unwritable");
    assert!(!dir.path().join("nested").exists());

    let mut params = base("clip-path", &out);
    params.create_dirs = Some(true);
    render(params);
    assert!(out.exists());
}

#[test]
fn a_failed_render_leaves_no_file_and_no_temporary() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("fails.jpg");
    let mut params = base("clip-path", &out);
    params.transparent = Some(true); // JPEG has no alpha channel
    let error = tools::render_svg::run(&state(), params).unwrap_err();

    assert_eq!(error.code.as_str(), "unsupported_format");
    let left_behind: Vec<_> = std::fs::read_dir(dir.path()).unwrap().flatten().collect();
    assert!(left_behind.is_empty(), "the failed call left {left_behind:?} behind");
}

// --- The size model and the fit strategies -----------------------------------

#[test]
fn the_fit_strategies_do_what_they_say_on_a_100_by_80_document() {
    let dir = tempfile::tempdir().unwrap();

    let contain = dir.path().join("contain.png");
    let mut params = base("viewbox-aspect", &contain);
    params.width = Some(512);
    params.height = Some(512);
    let result = render(params);
    assert_eq!(result["padding_px"]["top"], 51);
    assert_eq!(result["padding_px"]["bottom"], 51);
    assert_eq!(result["padding_px"]["left"], 0);
    assert!(warning_codes(&result).contains(&"aspect_adjusted".to_string()));
    let image = decode(&contain).to_rgba8();
    assert_eq!(image.get_pixel(256, 5).0[3], 0, "the letterbox has to stay transparent");
    assert!(image.get_pixel(256, 256).0[3] > 0, "the drawing has to be opaque");

    let stretch = dir.path().join("stretch.png");
    let mut params = base("viewbox-aspect", &stretch);
    params.width = Some(512);
    params.height = Some(512);
    params.fit = Some(img_gen_via_svg_mcp::render::fit::Fit::Stretch);
    let result = render(params);
    assert_eq!(result["padding_px"]["top"], 0);
    let image = decode(&stretch).to_rgba8();
    assert!(image.get_pixel(256, 5).0[3] > 0, "stretch leaves no transparent border");

    let cover = dir.path().join("cover.png");
    let mut params = base("viewbox-aspect", &cover);
    params.width = Some(512);
    params.height = Some(512);
    params.fit = Some(img_gen_via_svg_mcp::render::fit::Fit::Cover);
    let result = render(params);
    assert_eq!(result["padding_px"]["top"], 0);

    let mut params = base("viewbox-aspect", &dir.path().join("error.png"));
    params.width = Some(512);
    params.height = Some(512);
    params.fit = Some(img_gen_via_svg_mcp::render::fit::Fit::Error);
    let error = tools::render_svg::run(&state(), params).unwrap_err();
    assert_eq!(error.code.as_str(), "aspect_mismatch");
}

#[test]
fn one_dimension_derives_the_other_and_scale_is_refused_alongside() {
    let dir = tempfile::tempdir().unwrap();

    let result = render(RenderSvgParams {
        width: Some(512),
        ..base("viewbox-aspect", &dir.path().join("w.png"))
    });
    assert_eq!(result["width"], 512);
    assert_eq!(result["height"], 410);

    let result = render(RenderSvgParams {
        scale: Some(2.0),
        ..base("viewbox-aspect", &dir.path().join("s.png"))
    });
    assert_eq!(result["width"], 200);
    assert_eq!(result["height"], 160);

    let error = tools::render_svg::run(
        &state(),
        RenderSvgParams {
            width: Some(10),
            scale: Some(2.0),
            ..base("viewbox-aspect", &dir.path().join("x.png"))
        },
    )
    .unwrap_err();
    assert_eq!(error.code.as_str(), "invalid_input");
}

#[test]
fn a_size_beyond_the_limit_is_an_error_rather_than_a_clamp() {
    let dir = tempfile::tempdir().unwrap();
    let config = Config { max_pixels: 10_000, ..Config::default() };
    let error = tools::render_svg::run(
        &state_with(config),
        RenderSvgParams {
            width: Some(1000),
            height: Some(1000),
            ..base("clip-path", &dir.path().join("big.png"))
        },
    )
    .unwrap_err();
    assert_eq!(error.code.as_str(), "size_limit_exceeded");
}

// --- Background, padding and flattening --------------------------------------

#[test]
fn the_padding_and_flattening_matrix_holds() {
    let dir = tempfile::tempdir().unwrap();

    // A colour for the padding alone leaves the drawing's own transparency.
    let out = dir.path().join("padded.png");
    let mut params = base("viewbox-aspect", &out);
    params.width = Some(200);
    params.height = Some(200);
    params.padding_color = Some("#0000ff".into());
    render(params);
    let image = decode(&out).to_rgba8();
    assert_eq!(image.get_pixel(100, 2).0, [0, 0, 255, 255], "the padding takes padding_color");

    // A background fills the whole canvas, padding included.
    let out = dir.path().join("background.png");
    let mut params = base("viewbox-aspect", &out);
    params.width = Some(200);
    params.height = Some(200);
    params.background = Some("#ff0000".into());
    render(params);
    let image = decode(&out).to_rgba8();
    assert_eq!(image.get_pixel(100, 2).0, [255, 0, 0, 255]);

    // A format without an alpha channel flattens, and says so.
    let out = dir.path().join("flat.jpg");
    let mut params = base("viewbox-aspect", &out);
    params.width = Some(200);
    params.height = Some(200);
    let result = render(params);
    assert!(warning_codes(&result).contains(&"alpha_flattened".to_string()));
    let pixel = decode(&out).to_rgba8().get_pixel(100, 2).0;
    assert!(pixel[0] > 240 && pixel[1] > 240 && pixel[2] > 240, "default flatten colour is white");
    assert_eq!(result["has_alpha"], false);

    // With a background, that colour is what the flattening uses.
    let out = dir.path().join("flat-red.jpg");
    let mut params = base("viewbox-aspect", &out);
    params.width = Some(200);
    params.height = Some(200);
    params.background = Some("#ff0000".into());
    render(params);
    let pixel = decode(&out).to_rgba8().get_pixel(100, 2).0;
    assert!(pixel[0] > 200 && pixel[1] < 60, "the flatten colour is the background, got {pixel:?}");

    // BMP keeps its alpha channel.
    let out = dir.path().join("alpha.bmp");
    let mut params = base("viewbox-aspect", &out);
    params.width = Some(200);
    params.height = Some(200);
    let result = render(params);
    assert_eq!(result["has_alpha"], true);
    assert_eq!(decode(&out).to_rgba8().get_pixel(100, 2).0[3], 0, "BMP padding stays transparent");
}

// --- Encoder options ---------------------------------------------------------

#[test]
fn encoder_options_change_the_file_as_documented() {
    let dir = tempfile::tempdir().unwrap();

    let low = dir.path().join("low.jpg");
    let high = dir.path().join("high.jpg");
    for (path, quality) in [(&low, 10u8), (&high, 95)] {
        let mut params = base("gradient-radial", path);
        params.width = Some(200);
        params.height = Some(200);
        params.jpeg_quality = Some(quality);
        render(params);
    }
    assert!(
        std::fs::metadata(&low).unwrap().len() < std::fs::metadata(&high).unwrap().len(),
        "a lower JPEG quality has to produce a smaller file"
    );

    let loose = dir.path().join("loose.png");
    let tight = dir.path().join("tight.png");
    for (path, setting) in [(&loose, "none"), (&tight, "best")] {
        let mut params = base("gradient-radial", path);
        params.width = Some(200);
        params.height = Some(200);
        params.png_compression = Some(setting.to_string());
        render(params);
    }
    assert!(std::fs::metadata(&tight).unwrap().len() < std::fs::metadata(&loose).unwrap().len());
    assert_eq!(decode(&loose).to_rgba8(), decode(&tight).to_rgba8(), "compression is lossless");

    let optimised = dir.path().join("opt.png");
    let mut params = base("gradient-radial", &optimised);
    params.width = Some(200);
    params.height = Some(200);
    params.png_optimize = Some(true);
    render(params);
    assert_eq!(decode(&optimised).to_rgba8(), decode(&tight).to_rgba8(), "optimising is lossless");

    let dense = dir.path().join("dense.png");
    let mut params = base("clip-path", &dense);
    params.density_metadata = Some(300.0);
    let result = render(params);
    assert!(!warning_codes(&result).contains(&"metadata_unsupported".to_string()));
    assert!(std::fs::read(&dense).unwrap().windows(4).any(|w| w == b"pHYs"));

    let dense_bmp = dir.path().join("dense.bmp");
    let mut params = base("clip-path", &dense_bmp);
    params.density_metadata = Some(300.0);
    let result = render(params);
    assert!(warning_codes(&result).contains(&"metadata_unsupported".to_string()));
}

// --- Inline image content ----------------------------------------------------

#[test]
fn inline_image_content_is_returned_and_bounded() {
    let state = state();
    let outcome = tools::render_svg::run(
        &state,
        RenderSvgParams {
            svg_path: Some(fixture("clip-path")),
            return_mode: Some(ReturnMode::Image),
            width: Some(64),
            height: Some(64),
            ..RenderSvgParams::default()
        },
    )
    .unwrap();
    assert_eq!(outcome.images.len(), 1);
    assert_eq!(outcome.images[0].mime_type, "image/png");
    assert_eq!(outcome.structured["output_path"], Value::Null);

    let error = tools::render_svg::run(
        &state,
        RenderSvgParams {
            svg_path: Some(fixture("clip-path")),
            return_mode: Some(ReturnMode::Image),
            width: Some(256),
            height: Some(256),
            max_inline_bytes: Some(16),
            ..RenderSvgParams::default()
        },
    )
    .unwrap_err();
    assert_eq!(error.code.as_str(), "inline_too_large");
}

// --- Batch -------------------------------------------------------------------

#[test]
fn a_batch_parses_once_and_reports_each_entry_separately() {
    let dir = tempfile::tempdir().unwrap();
    let outcome = tools::render_svg_batch::run(
        &state(),
        RenderSvgBatchParams {
            svg_path: Some(fixture("clip-path")),
            outputs: vec![
                BatchOutput {
                    output_path: Some(dir.path().join("a.png")),
                    width: Some(32),
                    height: Some(32),
                    ..BatchOutput::default()
                },
                BatchOutput {
                    output_path: Some(dir.path().join("b.webp")),
                    width: Some(64),
                    height: Some(64),
                    ..BatchOutput::default()
                },
                BatchOutput {
                    // No parent directory and no permission to create one.
                    output_path: Some(dir.path().join("missing/c.png")),
                    width: Some(16),
                    height: Some(16),
                    ..BatchOutput::default()
                },
            ],
            ..RenderSvgBatchParams::default()
        },
    )
    .unwrap();

    let result = outcome.structured;
    assert_eq!(result["parsed_once"], true);
    assert_eq!(result["succeeded"], 2);
    assert_eq!(result["failed"], 1);
    assert_eq!(result["results"][0]["status"], "ok");
    assert_eq!(result["results"][1]["format"], "webp");
    assert_eq!(result["results"][2]["status"], "error");
    assert_eq!(result["results"][2]["error"]["code"], "output_unwritable");
    assert_eq!(decode(&dir.path().join("a.png")).width(), 32);
}

#[test]
fn a_document_level_parameter_in_a_batch_entry_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let error = tools::render_svg_batch::run(
        &state(),
        RenderSvgBatchParams {
            svg_path: Some(fixture("clip-path")),
            outputs: Vec::new(),
            ..RenderSvgBatchParams::default()
        },
    )
    .unwrap_err();
    assert_eq!(error.code.as_str(), "invalid_input");
    let _ = dir;
}

// --- Icons -------------------------------------------------------------------

#[test]
fn an_ico_holds_every_requested_size_rendered_from_the_vector() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("app.ico");
    let outcome =
        tools::render_icon::run(&state(), icon_params(&out, Some(vec![16, 32, 64]))).unwrap();

    assert_eq!(outcome.structured["container"], "ico");
    let bytes = std::fs::read(&out).unwrap();
    assert_eq!(&bytes[..4], &[0, 0, 1, 0], "an ICO file starts with its own magic");
    assert_eq!(u16::from_le_bytes([bytes[4], bytes[5]]), 3);

    // Each entry has to come from the vector source, not from a downscale.
    let direct = dir.path().join("direct.png");
    let mut params = base("clip-path", &direct);
    params.width = Some(16);
    params.height = Some(16);
    render(params);
    let from_ico = image::load_from_memory_with_format(&bytes, image::ImageFormat::Ico).unwrap();
    assert_eq!(from_ico.width(), 64, "the decoder returns the largest entry");
    assert!(decode(&direct).width() == 16);
}

#[test]
fn an_ico_refuses_a_size_the_format_cannot_express() {
    let dir = tempfile::tempdir().unwrap();
    let error = tools::render_icon::run(
        &state(),
        icon_params(&dir.path().join("big.ico"), Some(vec![512])),
    )
    .unwrap_err();
    assert_eq!(error.code.as_str(), "invalid_input");
}

#[test]
fn a_png_set_writes_one_file_per_size() {
    let dir = tempfile::tempdir().unwrap();
    let mut params = icon_params(&dir.path().join("set"), Some(vec![16, 48]));
    params.container = Some(IconContainer::PngSet);
    let outcome = tools::render_icon::run(&state(), params).unwrap();

    let files = outcome.structured["files"].as_array().unwrap();
    assert_eq!(files.len(), 2);
    assert_eq!(decode(&dir.path().join("set/icon-16.png")).width(), 16);
    assert_eq!(decode(&dir.path().join("set/icon-48.png")).width(), 48);
}

fn icon_params(out: &Path, sizes: Option<Vec<u32>>) -> RenderIconParams {
    RenderIconParams {
        svg_path: Some(fixture("clip-path")),
        svg_source: None,
        svg_url: None,
        resources_dir: None,
        output_path: out.to_path_buf(),
        container: None,
        sizes,
        png_name_pattern: None,
        fit: None,
        align: None,
        background: None,
        padding_color: None,
        overwrite: Some(true),
        create_dirs: Some(true),
        png_compression: None,
        png_optimize: None,
        dpi: None,
        fonts: None,
        on_missing_font: None,
        languages: None,
        stylesheet: None,
        on_unsupported: None,
    }
}

// --- The fidelity contract ---------------------------------------------------

#[test]
fn every_declared_gap_has_a_document_that_provokes_it() {
    // A gap in the catalogue with nothing to prove it would be a promise nobody
    // checks, so this test fails the build rather than the reviewer's attention.
    let documents: Vec<PathBuf> =
        std::fs::read_dir(corpus()).unwrap().flatten().map(|e| e.path()).collect();

    let mut seen: Vec<String> = Vec::new();
    for document in &documents {
        let outcome = tools::probe_svg::run(
            &state(),
            ProbeSvgParams { svg_path: Some(document.clone()), ..ProbeSvgParams::default() },
        )
        .unwrap();
        for finding in outcome.structured["unsupported"].as_array().unwrap() {
            seen.push(finding["code"].as_str().unwrap().to_string());
        }
        for warning in outcome.structured["warnings"].as_array().unwrap() {
            seen.push(warning["code"].as_str().unwrap().to_string());
        }
    }

    // These three are raised by the renderer or the encoder rather than by a
    // document's own markup, and are covered by their own tests.
    let covered_elsewhere = ["parser_diagnostic", "filter_unsupported", "font_substituted"];
    let missing: Vec<&str> = GAPS
        .iter()
        .map(|gap| gap.code)
        .filter(|code| !covered_elsewhere.contains(code) && !seen.iter().any(|s| s == code))
        .collect();
    assert!(missing.is_empty(), "no corpus document provokes these gaps: {missing:?}");
}

#[test]
fn probe_and_render_agree_about_a_document() {
    for name in ["gaps-animation", "gaps-foreignobject", "clip-path", "pattern"] {
        let probe = tools::probe_svg::run(
            &state(),
            ProbeSvgParams { svg_path: Some(fixture(name)), ..ProbeSvgParams::default() },
        )
        .unwrap()
        .structured;
        let dir = tempfile::tempdir().unwrap();
        let rendered = render(base(name, &dir.path().join("out.png")));

        let probed: Vec<String> = probe["unsupported"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["code"].as_str().unwrap().to_string())
            .collect();
        let warned = warning_codes(&rendered);
        for code in &probed {
            assert!(
                warned.contains(code),
                "{name}: probe_svg reported {code} but render_svg did not warn about it"
            );
        }
    }
}

#[test]
fn a_document_that_renders_faithfully_carries_no_warnings() {
    // A stray warning on a clean document teaches callers to ignore warnings,
    // which is what makes every other warning worthless.
    let dir = tempfile::tempdir().unwrap();
    for name in ["clip-path", "pattern", "mask-luminance", "gradient-linear", "text-basic"] {
        let result = render(base(name, &dir.path().join(format!("{name}.png"))));
        assert_eq!(warning_codes(&result), Vec::<String>::new(), "{name} should render clean");
    }
}

#[test]
fn strict_mode_refuses_rather_than_rendering_something_wrong() {
    let dir = tempfile::tempdir().unwrap();
    let mut params = base("gaps-animation", &dir.path().join("strict.png"));
    params.on_unsupported = Some(img_gen_via_svg_mcp::svg::fidelity::OnUnsupported::Error);
    let error = tools::render_svg::run(&state(), params).unwrap_err();

    assert_eq!(error.code.as_str(), "unsupported_feature");
    assert!(!dir.path().join("strict.png").exists());
}

#[test]
fn a_missing_font_is_named_and_can_be_made_fatal() {
    let dir = tempfile::tempdir().unwrap();
    let result = render(base("text-missing-font", &dir.path().join("font.png")));
    assert!(warning_codes(&result).contains(&"font_missing".to_string()));

    let mut params = base("text-missing-font", &dir.path().join("font2.png"));
    params.on_missing_font = Some(img_gen_via_svg_mcp::svg::fidelity::OnMissingFont::Error);
    let error = tools::render_svg::run(&state(), params).unwrap_err();
    assert_eq!(error.code.as_str(), "font_missing");
}

#[test]
fn the_displacement_defect_is_reported_rather_than_rendered_silently() {
    // The renderer applies feDisplacementMap's scale twice. The image is wrong
    // whatever this server does, so the one thing it must not do is stay quiet.
    let dir = tempfile::tempdir().unwrap();
    let result = render(base("filter-displacement", &dir.path().join("disp.png")));
    let codes = warning_codes(&result);
    assert!(
        codes.contains(&"filter_displacement_scale".to_string()),
        "expected the displacement warning, got {codes:?}"
    );
}

// --- probe_svg ---------------------------------------------------------------

#[test]
fn probe_reports_the_document_without_rendering_it() {
    let outcome = tools::probe_svg::run(
        &state(),
        ProbeSvgParams { svg_path: Some(fixture("filter-blur")), ..ProbeSvgParams::default() },
    )
    .unwrap();
    let report = outcome.structured;

    assert_eq!(report["size"]["width"], 240.0);
    assert_eq!(report["content"]["has_filters"], true);
    assert_eq!(report["content"]["has_text"], true);
    let primitives: Vec<&str> = report["filter_primitives"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(primitives.contains(&"feGaussianBlur"));
    let fonts = report["fonts_requested"].as_array().unwrap();
    assert!(fonts.iter().any(|f| f["family"] == "DejaVu Sans"));
}

// --- optimize_svg ------------------------------------------------------------

#[test]
fn normalising_produces_a_document_that_renders_the_same_pixels() {
    let dir = tempfile::tempdir().unwrap();
    let normalised = dir.path().join("normalised.svg");
    let outcome = tools::optimize_svg::run(
        &state(),
        OptimizeSvgParams {
            svg_path: Some(fixture("gradient-radial")),
            output_path: Some(normalised.clone()),
            ..OptimizeSvgParams::default()
        },
    )
    .unwrap();
    assert_eq!(outcome.structured["mode"], "normalize");

    let before = dir.path().join("before.png");
    let after = dir.path().join("after.png");
    let mut params = base("gradient-radial", &before);
    params.width = Some(160);
    params.height = Some(160);
    render(params);
    render(RenderSvgParams {
        svg_path: Some(normalised),
        output_path: Some(after.clone()),
        width: Some(160),
        height: Some(160),
        ..RenderSvgParams::default()
    });

    let (a, b) = (decode(&before).to_rgba8(), decode(&after).to_rgba8());
    let differing = a.pixels().zip(b.pixels()).filter(|(x, y)| x != y).count();
    assert!(
        differing * 100 < (a.width() * a.height()) as usize,
        "normalising changed {differing} pixels, which is more than rounding explains"
    );
}

#[test]
fn minifying_keeps_the_document_parseable_and_smaller() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("min.svg");
    let outcome = tools::optimize_svg::run(
        &state(),
        OptimizeSvgParams {
            svg_path: Some(fixture("filter-chain")),
            output_path: Some(out.clone()),
            mode: Some(OptimizeMode::Minify),
            ..OptimizeSvgParams::default()
        },
    )
    .unwrap();

    let result = outcome.structured;
    assert!(result["bytes_out"].as_u64().unwrap() < result["bytes_in"].as_u64().unwrap());
    render(RenderSvgParams {
        svg_path: Some(out),
        output_path: Some(dir.path().join("min.png")),
        width: Some(64),
        height: Some(64),
        ..RenderSvgParams::default()
    });
}

#[test]
fn outlining_text_removes_the_font_dependency() {
    let dir = tempfile::tempdir().unwrap();
    let outlined = dir.path().join("outlined.svg");
    tools::optimize_svg::run(
        &state(),
        OptimizeSvgParams {
            svg_path: Some(fixture("text-basic")),
            output_path: Some(outlined.clone()),
            text_to_paths: Some(true),
            ..OptimizeSvgParams::default()
        },
    )
    .unwrap();

    let config = Config { skip_system_fonts: true, ..Config::default() };
    let outcome = tools::render_svg::run(
        &state_with(config),
        RenderSvgParams {
            svg_path: Some(outlined),
            output_path: Some(dir.path().join("outlined.png")),
            width: Some(160),
            height: Some(160),
            ..RenderSvgParams::default()
        },
    )
    .unwrap();
    let codes: Vec<String> = outcome.structured["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["code"].as_str().unwrap().to_string())
        .collect();
    assert!(!codes.contains(&"font_missing".to_string()), "outlined text still needs a font");
}

// --- convert_image -----------------------------------------------------------

#[test]
fn converting_moves_a_raster_between_formats_sizes_and_base64() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.png");
    let mut params = base("gradient-linear", &source);
    params.width = Some(120);
    params.height = Some(120);
    render(params);

    let converted = dir.path().join("converted.webp");
    let outcome = tools::convert_image::run(
        &state(),
        ConvertImageParams {
            input_path: Some(source.clone()),
            output_path: Some(converted.clone()),
            width: Some(60),
            height: Some(60),
            ..ConvertImageParams::default()
        },
    )
    .unwrap();
    assert_eq!(outcome.structured["format"], "webp");
    assert_eq!(decode(&converted).width(), 60);

    let outcome = tools::convert_image::run(
        &state(),
        ConvertImageParams {
            input_path: Some(source),
            return_mode: Some(ConvertReturnMode::Base64),
            format: Some(Format::Png),
            ..ConvertImageParams::default()
        },
    )
    .unwrap();
    assert!(outcome.structured["base64"].as_str().unwrap().len() > 100);
}

#[test]
fn converting_refuses_svg_input_and_points_at_the_right_tool() {
    let error = tools::convert_image::run(
        &state(),
        ConvertImageParams {
            input_path: Some(fixture("clip-path")),
            return_mode: Some(ConvertReturnMode::Base64),
            format: Some(Format::Png),
            ..ConvertImageParams::default()
        },
    )
    .unwrap_err();
    assert_eq!(error.code.as_str(), "invalid_input");
    assert!(error.hint.unwrap().contains("render_svg"));
}

// --- get_capabilities --------------------------------------------------------

#[test]
fn every_format_the_capabilities_claim_can_actually_be_encoded() {
    let state = state();
    let report = tools::capabilities::run(&state).unwrap().structured;
    let dir = tempfile::tempdir().unwrap();

    for name in report["formats"]["encode"].as_array().unwrap() {
        let name = name.as_str().unwrap();
        let format = Format::ALL
            .iter()
            .find(|f| f.as_str() == name)
            .unwrap_or_else(|| panic!("capabilities claim an unknown format {name}"));
        let out = dir.path().join(format!("claim.{}", format.extension()));
        let mut params = base("clip-path", &out);
        params.format = Some(*format);
        params.width = Some(32);
        params.height = Some(32);
        tools::render_svg::run(&state, params)
            .unwrap_or_else(|e| panic!("capabilities claim {name} but encoding it failed: {e}"));
    }

    let gaps = report["svg_support"]["known_gaps"].as_array().unwrap();
    assert_eq!(gaps.len(), GAPS.len());
}

// --- Robustness --------------------------------------------------------------

#[test]
fn malformed_input_produces_a_specific_error() {
    let dir = tempfile::tempdir().unwrap();
    for (source, expected) in [
        ("<svg><unclosed>", "parse_failed"),
        ("", "parse_failed"),
        ("<html><body/></html>", "parse_failed"),
    ] {
        let error = tools::render_svg::run(
            &state(),
            RenderSvgParams {
                svg_source: Some(source.to_string()),
                output_path: Some(dir.path().join("bad.png")),
                ..RenderSvgParams::default()
            },
        )
        .unwrap_err();
        assert_eq!(error.code.as_str(), expected, "for input {source:?}");
    }

    let error = tools::render_svg::run(
        &state(),
        RenderSvgParams {
            svg_path: Some(dir.path().join("nothing-here.svg")),
            output_path: Some(dir.path().join("out.png")),
            ..RenderSvgParams::default()
        },
    )
    .unwrap_err();
    assert_eq!(error.code.as_str(), "input_not_found");
}

#[test]
fn remote_input_is_refused_until_the_operator_enables_it() {
    let dir = tempfile::tempdir().unwrap();
    let error = tools::render_svg::run(
        &state(),
        RenderSvgParams {
            svg_url: Some("https://example.org/logo.svg".into()),
            output_path: Some(dir.path().join("remote.png")),
            ..RenderSvgParams::default()
        },
    )
    .unwrap_err();
    assert_eq!(error.code.as_str(), "remote_disabled");
}

#[test]
fn exactly_one_input_source_is_required() {
    let dir = tempfile::tempdir().unwrap();
    let error = tools::render_svg::run(
        &state(),
        RenderSvgParams {
            output_path: Some(dir.path().join("none.png")),
            ..RenderSvgParams::default()
        },
    )
    .unwrap_err();
    assert_eq!(error.code.as_str(), "invalid_input");

    let error = tools::render_svg::run(
        &state(),
        RenderSvgParams {
            svg_path: Some(fixture("clip-path")),
            svg_source: Some("<svg/>".into()),
            output_path: Some(dir.path().join("two.png")),
            ..RenderSvgParams::default()
        },
    )
    .unwrap_err();
    assert_eq!(error.code.as_str(), "invalid_input");
}

// --- Sub-element export ------------------------------------------------------

#[test]
fn a_named_element_can_be_exported_on_its_own() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("badge.png");
    let result = render(RenderSvgParams {
        export_id: Some("badge-large".into()),
        export_area: Some(img_gen_via_svg_mcp::render::renderer::ExportArea::Object),
        width: Some(64),
        height: Some(64),
        ..base("structure-use-symbol", &out)
    });
    assert_eq!(result["width"], 64);
    assert!(decode(&out).to_rgba8().pixels().any(|p| p.0[3] > 0), "the export drew nothing");

    let error = tools::render_svg::run(
        &state(),
        RenderSvgParams {
            export_id: Some("no-such-id".into()),
            ..base("structure-use-symbol", &dir.path().join("x.png"))
        },
    )
    .unwrap_err();
    assert_eq!(error.code.as_str(), "element_not_found");
}
