#[allow(dead_code)]
mod proprietary;

use axum::{
    extract::{Form, Query, State},
    http::{header, StatusCode},
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
    Json, Router,
};
use maud::{html, Markup, DOCTYPE};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    env, fs,
    path::{Path as FsPath, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
};
use tower_http::services::ServeDir;

const DEMO_VIEWER: &str = "demo-investor";

fn project_path(path: impl AsRef<FsPath>) -> PathBuf {
    // Prefer the folder the server runs from, so the app keeps working after it's moved.
    let base = env::current_dir()
        .ok()
        .filter(|dir| dir.join("public/assets").is_dir())
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    base.join(path)
}

#[derive(Clone)]
struct AppState {
    database: Arc<Mutex<HorseDatabase>>,
}

struct HorseDatabase {
    connection: Connection,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Profile {
    id: String,
    name: String,
    age: i64,
    breed: String,
    location: String,
    #[serde(rename = "distanceMiles")]
    distance_miles: i64,
    bio: String,
    #[serde(rename = "imageUrl")]
    image_url: String,
    #[serde(rename = "videoUrl")]
    video_url: Option<String>,
    #[serde(rename = "lookingFor")]
    looking_for: String,
    #[serde(rename = "likesYou")]
    likes_you: i64,
    featured: i64,
}

#[derive(Debug, Clone, Serialize)]
struct Match {
    id: String,
    name: String,
    age: i64,
    breed: String,
    location: String,
    #[serde(rename = "distanceMiles")]
    distance_miles: i64,
    bio: String,
    #[serde(rename = "imageUrl")]
    image_url: String,
    #[serde(rename = "videoUrl")]
    video_url: Option<String>,
    #[serde(rename = "lookingFor")]
    looking_for: String,
    #[serde(rename = "likesYou")]
    likes_you: i64,
    featured: i64,
    #[serde(rename = "matchedAt")]
    matched_at: String,
}

#[derive(Debug, Clone, Serialize)]
struct Stats {
    seen: i64,
    total: i64,
    likes: i64,
    matches: i64,
}

#[derive(Debug, Deserialize)]
struct SwipePayload {
    #[serde(rename = "profileId")]
    profile_id: Option<String>,
    direction: Option<String>,
}

impl HorseDatabase {
    fn open(path: &str) -> rusqlite::Result<Self> {
        if path != ":memory:" {
            if let Some(parent) = FsPath::new(path).parent() {
                let _ = fs::create_dir_all(parent);
            }
        }

        let connection = Connection::open(path)?;
        connection.execute_batch("PRAGMA foreign_keys = ON;")?;

        if path != ":memory:" {
            connection.execute_batch("PRAGMA journal_mode = WAL;")?;
        }

        let database = Self { connection };
        database.migrate()?;
        database.seed()?;
        Ok(database)
    }

    fn migrate(&self) -> rusqlite::Result<()> {
        self.connection.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS profiles (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                age INTEGER NOT NULL,
                breed TEXT NOT NULL,
                location TEXT NOT NULL,
                distance_miles INTEGER NOT NULL,
                bio TEXT NOT NULL,
                image_url TEXT NOT NULL,
                video_url TEXT,
                looking_for TEXT NOT NULL,
                likes_you INTEGER NOT NULL DEFAULT 0,
                featured INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS swipes (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                viewer_id TEXT NOT NULL,
                profile_id TEXT NOT NULL REFERENCES profiles(id),
                direction TEXT NOT NULL CHECK(direction IN ('pass', 'like')),
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                UNIQUE(viewer_id, profile_id)
            );

            CREATE TABLE IF NOT EXISTS matches (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                viewer_id TEXT NOT NULL,
                profile_id TEXT NOT NULL REFERENCES profiles(id),
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                UNIQUE(viewer_id, profile_id)
            );
            "#,
        )?;

        let mut columns = self.profile_columns()?;

        if !columns.iter().any(|column| column == "distance_miles") {
            if columns.iter().any(|column| column == "distance_km") {
                self.connection.execute_batch(
                    "ALTER TABLE profiles RENAME COLUMN distance_km TO distance_miles;",
                )?;
            } else {
                self.connection.execute_batch(
                    "ALTER TABLE profiles ADD COLUMN distance_miles INTEGER NOT NULL DEFAULT 0;",
                )?;
            }

            columns = self.profile_columns()?;
        }

        if columns.iter().any(|column| column == "distance_km") {
            self.connection.execute_batch(
                r#"
                UPDATE profiles
                SET distance_miles = distance_km
                WHERE distance_miles = 0;

                ALTER TABLE profiles DROP COLUMN distance_km;
                "#,
            )?;

            columns = self.profile_columns()?;
        }

        if !columns.iter().any(|column| column == "video_url") {
            self.connection
                .execute_batch("ALTER TABLE profiles ADD COLUMN video_url TEXT;")?;
        }

        Ok(())
    }

    fn profile_columns(&self) -> rusqlite::Result<Vec<String>> {
        let mut statement = self.connection.prepare("PRAGMA table_info(profiles)")?;
        let columns = statement
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<rusqlite::Result<Vec<_>>>();
        columns
    }

    fn seed(&self) -> rusqlite::Result<()> {
        let profiles = [
            (
                "maple",
                "Maple",
                7,
                "Palomino",
                "Palo Alto",
                3,
                "Sunset gallops, crunchy apples, and absolutely no emotional baggage trailers.",
                "/assets/maple.svg",
                "A stable relationship",
                1,
                1,
            ),
            (
                "diesel",
                "Diesel",
                9,
                "Clydesdale",
                "Cupertino",
                8,
                "Gentle giant. Can pull a wagon and carry a conversation.",
                "/assets/diesel.svg",
                "Someone who values horsepower",
                0,
                1,
            ),
            (
                "pixel",
                "Pixel",
                5,
                "Appaloosa",
                "Mountain View",
                11,
                "Spotted, chronically online, and looking for somebody to touch grass with.",
                "/assets/pixel.svg",
                "Low latency chemistry",
                1,
                0,
            ),
            (
                "athena",
                "Athena",
                8,
                "Andalusian",
                "San Jose",
                17,
                "Dressage over drama. Fluent in three gaits and two love languages.",
                "/assets/athena.svg",
                "Grace under pressure",
                0,
                0,
            ),
            (
                "rusty",
                "Rusty",
                11,
                "Quarter Horse",
                "Menlo Park",
                22,
                "Memory safe, fearless around tractors, occasionally refuses to borrow the saddle.",
                "/assets/rusty.svg",
                "Zero-cost companionship",
                1,
                0,
            ),
            (
                "mocha",
                "Mocha",
                6,
                "Thoroughbred",
                "San Francisco",
                36,
                "Fast on the track, slow to text back. My love language is premium hay.",
                "/assets/mocha.svg",
                "No commitment hurdles",
                0,
                0,
            ),
        ];

        let manifest: HashMap<String, (Option<String>, Option<String>)> = {
            let path = project_path("data/generated-media.json");

            if path.exists() {
                let text = fs::read_to_string(path).unwrap_or_else(|_| "{}".to_string());
                let raw: HashMap<String, HashMap<String, String>> =
                    serde_json::from_str(&text).unwrap_or_default();

                raw.into_iter()
                    .map(|(id, media)| {
                        (
                            id,
                            (
                                media.get("imageUrl").cloned(),
                                media.get("videoUrl").cloned(),
                            ),
                        )
                    })
                    .collect()
            } else {
                HashMap::new()
            }
        };

        for (
            id,
            name,
            age,
            breed,
            location,
            distance,
            bio,
            image,
            looking_for,
            likes_you,
            featured,
        ) in profiles
        {
            let (manifest_image, video) = manifest.get(id).cloned().unwrap_or((None, None));

            self.connection.execute(
                r#"
                INSERT INTO profiles (
                    id, name, age, breed, location, distance_miles, bio,
                    image_url, video_url, looking_for, likes_you, featured
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                ON CONFLICT(id) DO UPDATE SET
                    name = excluded.name,
                    age = excluded.age,
                    breed = excluded.breed,
                    location = excluded.location,
                    distance_miles = excluded.distance_miles,
                    bio = excluded.bio,
                    image_url = excluded.image_url,
                    video_url = excluded.video_url,
                    looking_for = excluded.looking_for,
                    likes_you = excluded.likes_you,
                    featured = excluded.featured
                "#,
                params![
                    id,
                    name,
                    age,
                    breed,
                    location,
                    distance,
                    bio,
                    manifest_image.unwrap_or_else(|| image.to_string()),
                    video,
                    looking_for,
                    likes_you,
                    featured
                ],
            )?;
        }

        Ok(())
    }

    fn profile_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Profile> {
        Ok(Profile {
            id: row.get("id")?,
            name: row.get("name")?,
            age: row.get("age")?,
            breed: row.get("breed")?,
            location: row.get("location")?,
            distance_miles: row.get("distanceMiles")?,
            bio: row.get("bio")?,
            image_url: row.get("imageUrl")?,
            video_url: row.get("videoUrl")?,
            looking_for: row.get("lookingFor")?,
            likes_you: row.get("likesYou")?,
            featured: row.get("featured")?,
        })
    }

    fn get_next_profile(&self, viewer: &str) -> rusqlite::Result<Option<Profile>> {
        self.connection
            .query_row(
                r#"
                SELECT p.id, p.name, p.age, p.breed, p.location,
                       p.distance_miles AS distanceMiles, p.bio,
                       p.image_url AS imageUrl, p.video_url AS videoUrl,
                       p.looking_for AS lookingFor, p.likes_you AS likesYou,
                       p.featured
                FROM profiles p
                WHERE NOT EXISTS (
                    SELECT 1
                    FROM swipes s
                    WHERE s.viewer_id = ?1
                      AND s.profile_id = p.id
                )
                ORDER BY p.featured DESC, p.rowid ASC
                LIMIT 1
                "#,
                params![viewer],
                Self::profile_from_row,
            )
            .optional()
    }

    fn get_profile(&self, id: &str) -> rusqlite::Result<Option<Profile>> {
        self.connection
            .query_row(
                r#"
                SELECT id, name, age, breed, location,
                       distance_miles AS distanceMiles, bio,
                       image_url AS imageUrl, video_url AS videoUrl,
                       looking_for AS lookingFor, likes_you AS likesYou,
                       featured
                FROM profiles
                WHERE id = ?1
                "#,
                params![id],
                Self::profile_from_row,
            )
            .optional()
    }

    fn stats(&self, viewer: &str) -> rusqlite::Result<Stats> {
        let total = self
            .connection
            .query_row("SELECT COUNT(*) FROM profiles", [], |row| row.get(0))?;

        let seen = self.connection.query_row(
            "SELECT COUNT(*) FROM swipes WHERE viewer_id = ?1",
            params![viewer],
            |row| row.get(0),
        )?;

        let likes = self.connection.query_row(
            "SELECT COUNT(*) FROM swipes WHERE viewer_id = ?1 AND direction = 'like'",
            params![viewer],
            |row| row.get(0),
        )?;

        let matches = self.connection.query_row(
            "SELECT COUNT(*) FROM matches WHERE viewer_id = ?1",
            params![viewer],
            |row| row.get(0),
        )?;

        Ok(Stats {
            seen,
            total,
            likes,
            matches,
        })
    }

    fn record_swipe(
        &mut self,
        viewer: &str,
        profile_id: &str,
        direction: &str,
    ) -> rusqlite::Result<bool> {
        let transaction = self.connection.transaction()?;

        let likes_you = transaction
            .query_row(
                "SELECT likes_you FROM profiles WHERE id = ?1",
                params![profile_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)?;

        transaction.execute(
            r#"
            INSERT INTO swipes (viewer_id, profile_id, direction)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(viewer_id, profile_id)
            DO UPDATE SET
                direction = excluded.direction,
                created_at = CURRENT_TIMESTAMP
            "#,
            params![viewer, profile_id, direction],
        )?;

        let matched = direction == "like" && likes_you == 1;

        if matched {
            transaction.execute(
                r#"
                INSERT OR IGNORE INTO matches (viewer_id, profile_id)
                VALUES (?1, ?2)
                "#,
                params![viewer, profile_id],
            )?;
        } else {
            transaction.execute(
                "DELETE FROM matches WHERE viewer_id = ?1 AND profile_id = ?2",
                params![viewer, profile_id],
            )?;
        }

        transaction.commit()?;
        Ok(matched)
    }

    fn list_matches(&self, viewer: &str) -> rusqlite::Result<Vec<Match>> {
        let mut statement = self.connection.prepare(
            r#"
            SELECT p.id, p.name, p.age, p.breed, p.location,
                   p.distance_miles, p.bio, p.image_url, p.video_url,
                   p.looking_for, p.likes_you, p.featured, m.created_at
            FROM matches m
            JOIN profiles p ON p.id = m.profile_id
            WHERE m.viewer_id = ?1
            ORDER BY m.created_at DESC
            "#,
        )?;

        let matches = statement
            .query_map(params![viewer], |row| {
                Ok(Match {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    age: row.get(2)?,
                    breed: row.get(3)?,
                    location: row.get(4)?,
                    distance_miles: row.get(5)?,
                    bio: row.get(6)?,
                    image_url: row.get(7)?,
                    video_url: row.get(8)?,
                    looking_for: row.get(9)?,
                    likes_you: row.get(10)?,
                    featured: row.get(11)?,
                    matched_at: row.get(12)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>();
        matches
    }

    fn reset(&mut self, viewer: &str) -> rusqlite::Result<()> {
        let transaction = self.connection.transaction()?;
        transaction.execute("DELETE FROM matches WHERE viewer_id = ?1", params![viewer])?;
        transaction.execute("DELETE FROM swipes WHERE viewer_id = ?1", params![viewer])?;
        transaction.commit()
    }
}

fn database(state: &AppState) -> MutexGuard<'_, HorseDatabase> {
    state.database.lock().expect("database mutex poisoned")
}

fn layout(title: &str, content: Markup, active: &str) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                meta name="theme-color" content="#efe3cf";
                title { (title) " · Horse Tinder" }
                link rel="preconnect" href="https://fonts.googleapis.com";
                link rel="preconnect" href="https://fonts.gstatic.com" crossorigin;
                link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Bitter:wght@500;700;800&family=Nunito+Sans:wght@400;600;700;800&display=swap";
                link rel="stylesheet" href="/assets/app.css";
                script src="/assets/swipe.js" defer {}
                script src="/assets/app.js" defer {}
            }
            body {
                div class="page-shell" {
                    header class="site-header" {
                        a class="brand" href="/" aria-label="Horse Tinder home" {
                            span class="brand-mark" { "H" }
                            span { "Horse Tinder" }
                        }
                        nav aria-label="Primary navigation" {
                            a
                                class=(if active == "discover" { "nav-link is-active" } else { "nav-link" })
                                href="/"
                            {
                                "Discover"
                            }
                            a
                                class=(if active == "matches" { "nav-link is-active" } else { "nav-link" })
                                href="/matches"
                            {
                                "Matches"
                            }
                        }
                    }
                    main { (content) }
                    footer {
                        "Built for horses who are tired of being put out to pasture."
                    }
                }
            }
        }
    }
}

fn progress(stats: &Stats) -> Markup {
    let percent = if stats.total == 0 {
        0
    } else {
        ((stats.seen * 100 + stats.total / 2) / stats.total).min(100)
    };

    html! {
        div
            class="progress-row"
            aria-label=(format!("{} of {} profiles reviewed", stats.seen, stats.total))
        {
            div class="progress-track" {
                span style=(format!("width: {}%", percent)) {}
            }
            span { (stats.seen) " / " (stats.total) }
        }
    }
}

fn discover_page(profile: Option<Profile>, stats: &Stats, match_name: Option<&str>) -> Markup {
    let toast = match_name.map(|name| {
        html! {
            aside class="match-toast" role="status" {
                span { "It is a match" }
                " You and " (name) " both chose hay."
            }
        }
    });

    let content = if let Some(profile) = profile {
        html! {
            section class="discover-layout" {
                div class="section-heading" {
                    p class="eyebrow" { "Fresh from the paddock" }
                    h1 { "Find your stable match." }
                    p { "One profile at a time. No endless feed. No horsing around." }
                }

                @if let Some(toast) = toast {
                    (toast)
                }

                article class="profile-card" data-swipe-card {
                    span
                        class="swipe-feedback swipe-feedback-pass"
                        data-swipe-feedback="pass"
                        aria-hidden="true"
                    {
                        "Pass"
                    }
                    span
                        class="swipe-feedback swipe-feedback-like"
                        data-swipe-feedback="like"
                        aria-hidden="true"
                    {
                        "Like"
                    }

                    div class="portrait-wrap" {
                        @if let Some(video_url) = profile.video_url.as_deref() {
                            video
                                autoplay
                                muted
                                loop
                                playsinline
                                poster=(profile.image_url.as_str())
                                aria-label=(format!("Video profile of {}", profile.name))
                            {
                                source src=(video_url) type="video/mp4";
                            }
                        } @else {
                            img
                                src=(profile.image_url.as_str())
                                alt=(format!("Portrait of {}", profile.name));
                        }

                        span class="distance" {
                            (profile.distance_miles) " miles away"
                        }
                    }

                    div class="profile-copy" {
                        div class="name-row" {
                            div {
                                h2 { (profile.name.as_str()) ", " (profile.age) }
                                p {
                                    (profile.breed.as_str())
                                    " · "
                                    (profile.location.as_str())
                                }
                                p class="local-time" data-local-time data-city=(profile.location.as_str()) {}
                            }

                            @if profile.featured != 0 {
                                span class="verified" title="Verified horse" { "✓" }
                            }
                        }

                        p class="bio" { (profile.bio.as_str()) }

                        div class="intent" {
                            span { "Looking for" }
                            (profile.looking_for.as_str())
                        }

                        form
                            class="swipe-actions"
                            method="post"
                            action="/swipes"
                            data-swipe-form
                        {
                            input
                                type="hidden"
                                name="profileId"
                                value=(profile.id.as_str());

                            button
                                class="button button-pass"
                                type="submit"
                                name="direction"
                                value="pass"
                            {
                                span aria-hidden="true" { "×" }
                                " Pass"
                            }

                            button
                                class="button button-like"
                                type="submit"
                                name="direction"
                                value="like"
                            {
                                span aria-hidden="true" { "♥" }
                                " Like"
                            }
                        }

                        p class="swipe-hint" {
                            span aria-hidden="true" { "←" }
                            " Swipe or use the arrow keys "
                            span aria-hidden="true" { "→" }
                        }
                    }
                }

                (progress(stats))
            }
        }
    } else {
        html! {
            section class="empty-state" {
                @if let Some(toast) = toast {
                    (toast)
                }

                span class="empty-icon" { "✓" }
                p class="eyebrow" { "You reached the fence" }
                h1 { "That is every horse nearby." }

                p {
                    "You reviewed "
                    (stats.total)
                    " profiles and made "
                    (stats.matches)
                    " "
                    @if stats.matches == 1 {
                        "match"
                    } @else {
                        "matches"
                    }
                    "."
                }

                div class="empty-actions" {
                    a class="button button-like" href="/matches" {
                        "View matches"
                    }
                    form method="post" action="/reset" {
                        button class="text-button" type="submit" {
                            "Start over"
                        }
                    }
                }
            }
        }
    };

    layout("Discover", content, "discover")
}

fn matches_page(matches: &[Match]) -> Markup {
    let cards = if matches.is_empty() {
        html! {
            div class="empty-inline" {
                span { "♡" }
                h2 { "No matches yet" }
                p { "Your perfect pasture partner may be one swipe away." }
                a class="button button-like" href="/" {
                    "Start discovering"
                }
            }
        }
    } else {
        html! {
            div class="match-grid" {
                @for profile in matches {
                    article class="match-card" {
                        img
                            src=(profile.image_url.as_str())
                            alt=(format!("Portrait of {}", profile.name));

                        div {
                            p class="eyebrow" data-matched-at=(profile.matched_at.as_str()) { "New match" }
                            h2 { (profile.name.as_str()) ", " (profile.age) }
                            p {
                                (profile.breed.as_str())
                                " · "
                                (profile.distance_miles)
                                " miles away"
                            }
                            span { (profile.looking_for.as_str()) }
                        }
                    }
                }
            }
        }
    };

    layout(
        "Matches",
        html! {
            section class="matches-layout" {
                div class="section-heading" {
                    p class="eyebrow" { "Mutual neighs" }
                    h1 { "Your matches" }
                    p {
                        (matches.len())
                        " "
                        @if matches.len() == 1 {
                            "horse likes"
                        } @else {
                            "horses like"
                        }
                        " you back."
                    }
                }
                (cards)
            }
        },
        "matches",
    )
}

fn not_found_page() -> Markup {
    layout(
        "Not found",
        html! {
            section class="empty-state" {
                span class="empty-icon" { "404" }
                h1 { "Wrong paddock." }
                p { "That page has bolted." }
                a class="button button-like" href="/" {
                    "Go home"
                }
            }
        },
        "discover",
    )
}

fn invalid_swipe_response() -> axum::response::Response {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({"error": "Invalid swipe"})),
    )
        .into_response()
}

fn profile_not_found_response() -> axum::response::Response {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({"error": "Profile not found"})),
    )
        .into_response()
}

fn validate_swipe(payload: SwipePayload) -> Result<(String, String), axum::response::Response> {
    let profile_id = payload.profile_id.ok_or_else(invalid_swipe_response)?;
    let direction = payload.direction.ok_or_else(invalid_swipe_response)?;

    if profile_id.is_empty() || (direction != "pass" && direction != "like") {
        return Err(invalid_swipe_response());
    }

    Ok((profile_id, direction))
}

async fn home(
    State(state): State<AppState>,
    Query(query): Query<HashMap<String, String>>,
) -> Html<String> {
    let (profile, stats) = {
        let db = database(&state);
        (
            db.get_next_profile(DEMO_VIEWER).unwrap_or(None),
            db.stats(DEMO_VIEWER).unwrap_or(Stats {
                seen: 0,
                total: 0,
                likes: 0,
                matches: 0,
            }),
        )
    };

    Html(discover_page(profile, &stats, query.get("match").map(String::as_str)).into_string())
}

async fn matches(State(state): State<AppState>) -> Html<String> {
    let values = {
        let db = database(&state);
        db.list_matches(DEMO_VIEWER).unwrap_or_default()
    };

    Html(matches_page(&values).into_string())
}

async fn reset(State(state): State<AppState>) -> Redirect {
    {
        let mut db = database(&state);
        let _ = db.reset(DEMO_VIEWER);
    }

    Redirect::to("/")
}

async fn form_swipe(
    State(state): State<AppState>,
    Form(payload): Form<SwipePayload>,
) -> impl IntoResponse {
    let (profile_id, direction) = match validate_swipe(payload) {
        Ok(values) => values,
        Err(response) => return response,
    };

    let result = {
        let mut db = database(&state);

        let profile = match db.get_profile(&profile_id) {
            Ok(Some(profile)) => profile,
            Ok(None) => return profile_not_found_response(),
            Err(_) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "Unable to load profile"})),
                )
                    .into_response();
            }
        };

        match db.record_swipe(DEMO_VIEWER, &profile_id, &direction) {
            Ok(matched) => (profile.name, matched),
            Err(_) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "Unable to record swipe"})),
                )
                    .into_response();
            }
        }
    };

    let (profile_name, matched) = result;
    let target = if matched {
        let query =
            serde_urlencoded::to_string([("match", profile_name.as_str())]).unwrap_or_default();
        format!("/?{query}")
    } else {
        "/".to_string()
    };

    (StatusCode::SEE_OTHER, [(header::LOCATION, target)]).into_response()
}

async fn api_profiles(State(state): State<AppState>) -> impl IntoResponse {
    let result = {
        let db = database(&state);
        db.get_next_profile(DEMO_VIEWER)
            .and_then(|current| db.stats(DEMO_VIEWER).map(|stats| (current, stats)))
    };

    match result {
        Ok((current, stats)) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "current": current,
                "stats": stats
            })),
        )
            .into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Unable to load profiles"})),
        )
            .into_response(),
    }
}

async fn api_matches(State(state): State<AppState>) -> impl IntoResponse {
    let result = {
        let db = database(&state);
        db.list_matches(DEMO_VIEWER)
    };

    match result {
        Ok(matches) => (
            StatusCode::OK,
            Json(serde_json::json!({"matches": matches})),
        )
            .into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Unable to load matches"})),
        )
            .into_response(),
    }
}

async fn api_swipe(
    State(state): State<AppState>,
    Json(payload): Json<SwipePayload>,
) -> impl IntoResponse {
    let (profile_id, direction) = match validate_swipe(payload) {
        Ok(values) => values,
        Err(response) => return response,
    };

    let result = {
        let mut db = database(&state);

        match db.get_profile(&profile_id) {
            Ok(Some(_)) => {}
            Ok(None) => return profile_not_found_response(),
            Err(_) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "Unable to load profile"})),
                )
                    .into_response();
            }
        }

        db.record_swipe(DEMO_VIEWER, &profile_id, &direction)
            .and_then(|matched| db.stats(DEMO_VIEWER).map(|stats| (matched, stats)))
    };

    match result {
        Ok((matched, stats)) => (
            StatusCode::CREATED,
            Json(serde_json::json!({
                "matched": matched,
                "stats": stats
            })),
        )
            .into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Unable to record swipe"})),
        )
            .into_response(),
    }
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "runtime": "rust"
    }))
}

async fn not_found() -> impl IntoResponse {
    (StatusCode::NOT_FOUND, Html(not_found_page().into_string()))
}

fn create_app(database: HorseDatabase) -> Router {
    let state = AppState {
        database: Arc::new(Mutex::new(database)),
    };

    Router::new()
        .route("/", get(home))
        .route("/matches", get(matches))
        .route("/reset", post(reset))
        .route("/swipes", post(form_swipe))
        .route("/api/profiles", get(api_profiles))
        .route("/api/matches", get(api_matches))
        .route("/api/swipes", post(api_swipe))
        .route("/health", get(health))
        .nest_service("/assets", ServeDir::new(project_path("public/assets")))
        .fallback(not_found)
        .with_state(state)
}

#[tokio::main]
async fn main() {
    let port = env::var("PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(4173);

    let path = env::var("HORSE_TINDER_DB").unwrap_or_else(|_| {
        project_path("data/horse-tinder.db")
            .to_string_lossy()
            .into_owned()
    });

    let database = HorseDatabase::open(&path).expect("failed to open horse tinder database");
    let app = create_app(database);

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port))
        .await
        .expect("failed to bind server");

    println!("Horse Tinder is grazing at http://localhost:{port}");

    axum::serve(listener, app).await.expect("server error");
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{to_bytes, Body},
        http::Request,
    };
    use serde_json::{json, Value};
    use tower::ServiceExt;

    fn test_app() -> Router {
        create_app(HorseDatabase::open(":memory:").expect("in-memory database should open"))
    }

    async fn response_text(response: axum::response::Response) -> String {
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body should be readable");
        String::from_utf8(bytes.to_vec()).expect("response body should be UTF-8")
    }

    async fn response_json(response: axum::response::Response) -> Value {
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body should be readable");
        serde_json::from_slice(&bytes).expect("response body should contain JSON")
    }

    #[tokio::test]
    async fn renders_a_real_profile_on_the_discovery_page() {
        let response = test_app()
            .oneshot(
                Request::builder()
                    .uri("/")
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");

        assert_eq!(response.status(), StatusCode::OK);

        let body = response_text(response).await;

        assert!(body.contains("Find your stable match"));
        assert!(body.contains("Maple, 7"));
        assert!(body.contains("Palomino · Palo Alto"));
        assert!(body.contains("3 miles away"));
        assert!(!body.contains("Byron Bay"));
        assert!(body.contains("data-swipe-card"));
        assert!(body.contains("/assets/swipe.js"));
    }

    #[tokio::test]
    async fn serves_the_stylesheet_from_the_crate_assets() {
        let response = test_app()
            .oneshot(
                Request::builder()
                    .uri("/assets/app.css")
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("asset request should succeed");

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CONTENT_TYPE], "text/css");
        assert!(response_text(response).await.contains("--accent: #b5452d"));
    }

    #[tokio::test]
    async fn records_a_like_and_creates_a_mutual_match() {
        let app = test_app();

        let swipe_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/swipes")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "profileId": "maple",
                            "direction": "like"
                        })
                        .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("swipe request should succeed");

        assert_eq!(swipe_response.status(), StatusCode::CREATED);

        let swipe_result = response_json(swipe_response).await;
        assert_eq!(swipe_result["matched"], json!(true));

        let matches_response = app
            .oneshot(
                Request::builder()
                    .uri("/api/matches")
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("matches request should succeed");

        assert_eq!(matches_response.status(), StatusCode::OK);

        let matches_result = response_json(matches_response).await;
        assert_eq!(
            matches_result["matches"]
                .as_array()
                .expect("matches should be an array")
                .len(),
            1
        );
        assert_eq!(matches_result["matches"][0]["id"], json!("maple"));
    }

    #[tokio::test]
    async fn rejects_malformed_swipe_requests() {
        let response = test_app()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/swipes")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "profileId": "maple",
                            "direction": "maybe"
                        })
                        .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            response_json(response).await,
            json!({"error": "Invalid swipe"})
        );
    }

    #[tokio::test]
    async fn rejects_swipes_with_missing_fields() {
        let response = test_app()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/swipes")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(json!({"profileId": "maple"}).to_string()))
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            response_json(response).await,
            json!({"error": "Invalid swipe"})
        );
    }

    #[tokio::test]
    async fn exposes_the_runtime_health_check() {
        let response = test_app()
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response_json(response).await,
            json!({
                "status": "ok",
                "runtime": "rust"
            })
        );
    }
}
