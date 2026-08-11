//! 静态文件服务路由
//!
//! 对应 Python FastAPI 静态挂载逻辑。

use actix_files::Files;
use actix_web::{web, HttpRequest, HttpResponse};
use std::path::{Path, PathBuf};

use crate::error::ApiError;

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(4)
        .expect("api crate should be nested under services/api-server/crates/api")
        .to_path_buf()
}

fn frontend_dir() -> PathBuf {
    project_root().join("frontend")
}

fn frontend_release_dir_candidates() -> [PathBuf; 2] {
    let frontend_dir = frontend_dir();
    [frontend_dir.join("vue-app").join("dist"), frontend_dir.join("dist")]
}

fn first_matching_path<T, I, F>(candidates: I, predicate: F) -> Option<T>
where
    I: IntoIterator<Item = T>,
    F: Fn(&T) -> bool,
{
    candidates.into_iter().find(predicate)
}

fn preferred_frontend_release_dir() -> Option<PathBuf> {
    first_matching_path(frontend_release_dir_candidates(), |path| path.exists())
}

fn legacy_frontend_archive_dir() -> PathBuf {
    project_root()
        .join("frontend")
        .join("backup")
        .join("legacy-frontend-archive")
}

fn legacy_frontend_root_dir() -> PathBuf {
    let legacy_archive = legacy_frontend_archive_dir();
    if legacy_archive.exists() {
        legacy_archive
    } else {
        frontend_dir()
    }
}

fn pics_dir() -> PathBuf {
    project_root().join("pics")
}

fn ai_static_dir() -> PathBuf {
    frontend_dir().join("static").join("ai")
}

fn resolve_release_file<F>(release_dir_candidates: [PathBuf; 2], relative_path: &str, is_file: F) -> Option<PathBuf>
where
    F: Fn(&Path) -> bool,
{
    first_matching_path(
        release_dir_candidates.into_iter().map(|dir| dir.join(relative_path)),
        |path| is_file(path.as_path()),
    )
}

fn find_release_file(relative_path: &str) -> Option<PathBuf> {
    resolve_release_file(frontend_release_dir_candidates(), relative_path, Path::is_file)
}

fn resolve_frontend_subdir<FR, FD>(
    release_dir_candidates: [PathBuf; 2],
    release_subdir: &str,
    legacy_root_dir: PathBuf,
    legacy_subdir: &str,
    release_root_exists: FR,
    dir_exists: FD,
) -> Option<PathBuf>
where
    FR: Fn(&Path) -> bool,
    FD: Fn(&Path) -> bool,
{
    let legacy_path = legacy_root_dir.join(legacy_subdir);

    if let Some(release_root) = first_matching_path(release_dir_candidates, |path| release_root_exists(path.as_path()))
    {
        let release_path = release_root.join(release_subdir);
        if dir_exists(release_path.as_path()) {
            Some(release_path)
        } else {
            dir_exists(legacy_path.as_path()).then_some(legacy_path)
        }
    } else {
        dir_exists(legacy_path.as_path()).then_some(legacy_path)
    }
}

fn select_frontend_subdir(release_subdir: &str, legacy_subdir: &str) -> Option<PathBuf> {
    resolve_frontend_subdir(
        frontend_release_dir_candidates(),
        release_subdir,
        legacy_frontend_root_dir(),
        legacy_subdir,
        Path::exists,
        Path::is_dir,
    )
}

fn mount_files_if_dir_exists(cfg: &mut web::ServiceConfig, route: &str, dir: &Path) {
    if dir.is_dir() {
        cfg.service(Files::new(route, dir.to_path_buf()));
    }
}

async fn favicon(req: HttpRequest) -> Result<HttpResponse, ApiError> {
    let favicon_path = tokio::task::spawn_blocking(|| {
        preferred_frontend_release_dir()
            .map(|dir| dir.join("favicon.ico"))
            .filter(|path| path.is_file())
            .unwrap_or_else(|| legacy_frontend_root_dir().join("favicon.ico"))
    })
    .await
    .map_err(|_| ApiError::Internal("task join failed".into()))?;

    if tokio::fs::metadata(&favicon_path).await.is_ok() {
        actix_files::NamedFile::open_async(favicon_path)
            .await
            .map(|file| file.into_response(&req))
            .map_err(|e| ApiError::Internal(format!("Failed to read favicon: {e}")))
    } else {
        Err(ApiError::NotFound("Favicon not found".into()))
    }
}

async fn canonical_frontend_page(req: HttpRequest, page: web::Path<String>) -> Result<HttpResponse, ApiError> {
    let page = page.into_inner();
    // Prefer Vue release build; fall back to legacy archive
    let page_path = if let Some(release_path) = find_release_file(&page) {
        release_path
    } else {
        let legacy_path = legacy_frontend_root_dir().join("html").join(&page);
        if tokio::fs::metadata(&legacy_path)
            .await
            .map(|m| m.is_file())
            .unwrap_or(false)
        {
            legacy_path
        } else {
            return Err(ApiError::NotFound(format!("Frontend page not found: {page}")));
        }
    };

    actix_files::NamedFile::open_async(page_path)
        .await
        .map(|file| file.into_response(&req))
        .map_err(|e| ApiError::Internal(format!("Failed to read frontend page: {e}")))
}

/// 注册静态文件路由
pub fn configure(cfg: &mut web::ServiceConfig) {
    let legacy_frontend_root = legacy_frontend_root_dir();
    let pics_dir = pics_dir();

    cfg.route("/favicon.ico", web::get().to(favicon));
    cfg.route(
        "/frontend/{page:[A-Za-z0-9_-]+\\.html}",
        web::get().to(canonical_frontend_page),
    );

    if let Some(assets_dir) = select_frontend_subdir("assets", "assets") {
        mount_files_if_dir_exists(cfg, "/frontend/assets", &assets_dir);
    }
    if let Some(icons_dir) = select_frontend_subdir("icons", "icons") {
        mount_files_if_dir_exists(cfg, "/frontend/icons", &icons_dir);
    }
    if let Some(fonts_dir) = select_frontend_subdir("fonts", "fonts") {
        mount_files_if_dir_exists(cfg, "/frontend/fonts", &fonts_dir);
    }
    if let Some(images_dir) = select_frontend_subdir("images", "images") {
        mount_files_if_dir_exists(cfg, "/frontend/images", &images_dir);
    }

    mount_files_if_dir_exists(cfg, "/frontend/html", &legacy_frontend_root.join("html"));
    mount_files_if_dir_exists(cfg, "/frontend/js", &legacy_frontend_root.join("js"));
    mount_files_if_dir_exists(cfg, "/frontend/css", &legacy_frontend_root.join("css"));
    mount_files_if_dir_exists(cfg, "/frontend/static/ai", &ai_static_dir());
    mount_files_if_dir_exists(cfg, "/frontend/static", &legacy_frontend_root.join("static"));
    mount_files_if_dir_exists(cfg, "/frontend/vendor", &legacy_frontend_root.join("vendor"));
    mount_files_if_dir_exists(cfg, "/frontend/wasm_src", &legacy_frontend_root.join("wasm_src"));

    mount_files_if_dir_exists(cfg, "/static/ai", &ai_static_dir());
    mount_files_if_dir_exists(cfg, "/static", &legacy_frontend_root.join("static"));
    mount_files_if_dir_exists(cfg, "/css", &legacy_frontend_root.join("css"));

    if pics_dir.exists() {
        cfg.service(Files::new("/pics", pics_dir));
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ai_static_dir, first_matching_path, frontend_dir, frontend_release_dir_candidates, legacy_frontend_archive_dir,
        legacy_frontend_root_dir, project_root, resolve_frontend_subdir, resolve_release_file,
    };
    use std::collections::HashSet;
    use std::path::{Path, PathBuf};

    #[test]
    fn static_file_paths_resolve_from_project_root() {
        let root = project_root();
        assert!(root.ends_with(Path::new("flight-monitor-system")));
        assert_eq!(frontend_dir(), root.join("frontend"));
        assert_eq!(
            legacy_frontend_archive_dir(),
            root.join("frontend").join("backup").join("legacy-frontend-archive")
        );
        assert_eq!(
            frontend_release_dir_candidates(),
            [
                root.join("frontend").join("vue-app").join("dist"),
                root.join("frontend").join("dist"),
            ]
        );
        assert_eq!(ai_static_dir(), root.join("frontend").join("static").join("ai"));
    }

    #[test]
    fn first_matching_path_returns_first_candidate_that_matches() {
        let preferred = first_matching_path(frontend_release_dir_candidates(), |path| {
            path.ends_with(Path::new("frontend").join("vue-app").join("dist"))
        })
        .expect("a matching candidate should be selected");

        assert_eq!(preferred, project_root().join("frontend").join("vue-app").join("dist"));
    }

    #[test]
    fn resolve_release_file_uses_candidate_order_without_touching_real_dist() {
        let frontend_root = project_root().join("frontend");
        let fallback_release = frontend_root.join("dist");
        let expected = fallback_release.join("login.html");
        let existing_files = HashSet::from([expected.clone()]);

        let page = resolve_release_file(frontend_release_dir_candidates(), "login.html", |path| {
            existing_files.contains(path)
        })
        .expect("a mocked release file should be selected");

        assert_eq!(page, expected);
    }

    #[test]
    fn frontend_asset_dirs_fall_back_to_legacy_when_preferred_release_subdir_is_missing() {
        let root = project_root();
        let preferred_release = root.join("frontend").join("vue-app").join("dist");
        let fallback_release = root.join("frontend").join("dist");
        let legacy_root = root.join("frontend").join("backup").join("legacy-frontend-archive");
        let existing_roots = HashSet::from([preferred_release.clone(), fallback_release]);
        let existing_dirs = HashSet::from([legacy_root.join("icons")]);

        let icons_dir = resolve_frontend_subdir(
            frontend_release_dir_candidates(),
            "icons",
            legacy_root.clone(),
            "icons",
            |path| existing_roots.contains(path),
            |path| existing_dirs.contains(path),
        )
        .expect("legacy fallback should be selected");

        assert_eq!(icons_dir, legacy_root.join("icons"));

        assert_eq!(legacy_frontend_root_dir(), legacy_root);
    }

    #[test]
    fn frontend_asset_dirs_use_preferred_release_subdir_when_present() {
        let preferred_release = project_root().join("frontend").join("vue-app").join("dist");
        let release_icons = preferred_release.join("icons");
        let existing_roots = HashSet::from([preferred_release]);
        let existing_dirs = HashSet::from([release_icons.clone()]);

        let icons_dir = resolve_frontend_subdir(
            frontend_release_dir_candidates(),
            "icons",
            PathBuf::from("legacy-root"),
            "icons",
            |path| existing_roots.contains(path),
            |path| existing_dirs.contains(path),
        )
        .expect("preferred release subdir should be selected");

        assert_eq!(icons_dir, release_icons);
    }
}
