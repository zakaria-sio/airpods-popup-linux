// Pop-up AirPods pour LibrePods (Linux) — façon iPhone
// Licence : AGPL-3.0 (comme LibrePods : https://github.com/kavishdevar/librepods)
//
// Deux affichages :
//   - ouverture du boîtier : la carte façon iPhone (écouteurs + boîtier qui tournent,
//     cercles de batterie). Images : ~/.local/share/airpods_popup/iphone_ecouteurs/ et iphone_boitier/
//     -> fabriquées par outils/extraire_animation.py à partir de TA vidéo iPhone.
//   - clic sur l'icône LibrePods (POPUP_MANUEL=1) : écouteurs + batteries + modes d'écoute.
//     Images : ~/.local/share/airpods_popup/frames/ si tu as les tiennes, sinon iphone_ecouteurs/.
//
// Les infos (nom, batteries, écouteurs dans le boîtier, mode d'écoute) viennent du fichier
// ~/.local/share/airpods_popup/etat.txt, écrit par LibrePods (patchs 05 à 09).
// Les ordres (changer le mode d'écoute) partent dans commande.txt (lu par LibrePods, patch 09).
//
// Utilisation (normalement, c'est LibrePods qui la lance) :
//   POPUP_BOUCLE=1 airpods-popup                  -> pop-up d'ouverture du boîtier
//   POPUP_BOUCLE=1 POPUP_MANUEL=1 airpods-popup   -> pop-up du clic sur l'icône

use iced::widget::{column, container, image as img, mouse_area, row, text};
use iced::{
    Alignment, Border, Color, ContentFit, Element, Font, Length, Padding, Shadow, Size,
    Subscription, Task, Vector, font, window,
};
use std::path::PathBuf;
use std::time::Duration;

// ---------- Réglages du design ----------
const LARGEUR_FENETRE: f32 = 580.0;
const HAUTEUR_FENETRE: f32 = 520.0;
const LARGEUR_CARTE: f32 = 520.0;
const LARGEUR_BULLE: f32 = 500.0;  // fenêtre du clic sur l'icône
const HAUTEUR_BULLE: f32 = 560.0;
const LARGEUR_MAX_IMAGE: u32 = 600; // images réduites pour la mémoire, assez grandes pour rester nettes
const VERT: [u8; 3] = [0x34, 0xC7, 0x59];
const ROUGE: [u8; 3] = [0xFF, 0x45, 0x3A];
const DUREE_MAX_SECONDES: u64 = 120; // sécurité en mode boucle
const DUREE_MANUEL_SECONDES: u64 = 20; // pop-up ouverte au clic (sans toucher à rien)

fn fond_carte() -> Color { Color::from_rgb8(28, 28, 30) } // = fond des photos du boîtier
fn bord_carte() -> Color { Color::from_rgba8(255, 255, 255, 0.08) }
fn texte_principal() -> Color { Color::from_rgb8(0xF5, 0xF5, 0xF7) }
fn texte_secondaire() -> Color { Color::from_rgb8(0xA1, 0xA1, 0xA6) }
fn gras() -> Font { Font { weight: font::Weight::Bold, ..Font::DEFAULT } }
fn semi_gras() -> Font { Font { weight: font::Weight::Semibold, ..Font::DEFAULT } }

// ---------- Les données ----------
#[derive(Debug, Clone, Default, PartialEq)]
struct Etat {
    nom: String,
    mac: String, // adresse Bluetooth, pour le bouton "Connecter"
    gauche: i32, gauche_charge: bool,
    droite: i32, droite_charge: bool,
    boitier: i32, boitier_charge: bool,
    dans_boitier: i32, // 2 = les deux dedans, 1 = un seul, 0 = aucun
    en_oreille: i32,   // nombre d'écouteurs dans les oreilles
    ecoute: i32,       // mode d'écoute : 1 désactivé, 2 réduction du bruit, 3 transparence, 4 adaptatif
}

struct Popup {
    images: Vec<img::Handle>,  // tes images (la paire d'écouteurs qui tourne)
    index: usize,
    debut_boucle: usize, // tes images = une apparition puis une pose : on joue l'apparition une fois,
    fin_boucle: usize,   // puis on fait des allers-retours sur la partie "pose" (jamais la disparition)
    sens: i32,
    fps: u64,
    tourne_en_rond: bool, // images = une boucle de rotation (vidéo iPhone) : on la joue en rond
    boucle: bool,
    manuel: bool,        // ouverte par un clic sur l'icône : se ferme seule après 10 s
    ticks: u64,
    ticks_oreilles: u32, // depuis combien de temps les 2 écouteurs sont dans les oreilles
    vu_hors_oreilles: bool, // on ne ferme qu'au moment où on les MET (pas sur une vieille info)
    etat: Etat,
    piles: [img::Handle; 2],    // icône batterie : écouteurs, boîtier
    boitier_plein: img::Handle, // boîtier ouvert avec les écouteurs dedans
    boitier_ferme: img::Handle, // boîtier fermé (affiché quand les écouteurs sont sortis)
    film_boitier: Vec<img::Handle>, // le film "le couvercle s'ouvre" (dossier boitier_film/), s'il existe
    index_boitier: usize,
    apparition: f32,            // 0 -> 1 : fondu à l'ouverture
    mode: Mode,
    connecte: Option<bool>,     // None = on ne sait pas encore
    // Images découpées dans ta vidéo iPhone (une boucle de rotation de 6 s chacune)
    iph_ecouteurs: Vec<img::Handle>,
    iph_boitier: Vec<img::Handle>,
    index_iph: usize,
    anneaux: [img::Handle; 2],  // cercles de batterie façon iPhone : écouteurs, boîtier
    reglages_ouverts: bool,     // les boutons de mode d'écoute sont affichés
    // Mode qu'on vient de demander + moment de la demande : tant que les AirPods ne l'ont pas
    // confirmé (max 4 s), on garde l'affichage sur ce mode -> plus de bouton qui "saute"
    mode_en_attente: Option<(i32, std::time::Instant)>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Mode {
    Carte, // la pop-up de l'ouverture du boîtier (style iPhone)
    Bulle, // clic sur l'icône : tes 146 images + batterie + bouton Réglages (mode d'écoute)
}

fn mode_demande() -> Mode {
    match std::env::var("POPUP_MANUEL").as_deref() {
        Ok("1") => Mode::Bulle,
        _ => Mode::Carte,
    }
}

#[derive(Debug, Clone)]
enum Message {
    Tick,
    RelireEtat,
    Fermer,
    VerifierConnexion,
    Connexion(bool),
    Connecter,
    Reglages,        // affiche / cache les boutons de mode d'écoute
    ChoisirMode(u8), // demande à LibrePods de changer de mode
}

fn dossier_popup() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home).join(".local/share/airpods_popup")
}

// ---------- Lecture du fichier d'état ----------
fn lire_etat() -> Etat {
    let mut e = Etat {
        nom: "AirPods".into(), gauche: -1, droite: -1, boitier: -1, dans_boitier: 2,
        ..Default::default()
    };
    let Ok(contenu) = std::fs::read_to_string(dossier_popup().join("etat.txt")) else { return e };
    for ligne in contenu.lines() {
        let Some((cle, val)) = ligne.split_once('=') else { continue };
        let nombre = val.trim().parse::<i32>().unwrap_or(-1);
        match cle {
            "nom" if !val.trim().is_empty() => e.nom = val.trim().to_string(),
            "mac" => e.mac = val.trim().to_string(),
            "gauche" => e.gauche = nombre,
            "gauche_charge" => e.gauche_charge = nombre == 1,
            "droite" => e.droite = nombre,
            "droite_charge" => e.droite_charge = nombre == 1,
            "boitier" => e.boitier = nombre,
            "boitier_charge" => e.boitier_charge = nombre == 1,
            "dans_boitier" if nombre >= 0 => e.dans_boitier = nombre,
            "en_oreille" if nombre >= 0 => e.en_oreille = nombre,
            "ecoute" => e.ecoute = nombre,
            _ => {}
        }
    }
    e
}

// Batterie "des écouteurs" : comme sur iPhone, une seule valeur (la plus basse des deux)
fn niveau_ecouteurs(e: &Etat) -> (i32, bool) {
    let niveau = match (e.gauche, e.droite) {
        (g, d) if g >= 0 && d >= 0 => g.min(d),
        (g, d) => g.max(d), // une seule connue (ou aucune : -1)
    };
    (niveau, e.gauche_charge || e.droite_charge)
}

// ---------- Outils de dessin "pixel par pixel" ----------
// Pour chaque pixel, on calcule sa distance à une forme (négative = dedans).
// Ça permet des bords lisses (anti-crénelage) sans bibliothèque graphique.
fn dist_rect_arrondi(px: f32, py: f32, x: f32, y: f32, l: f32, h: f32, r: f32) -> f32 {
    let (cx, cy) = (x + l / 2.0, y + h / 2.0);
    let qx = (px - cx).abs() - (l / 2.0 - r);
    let qy = (py - cy).abs() - (h / 2.0 - r);
    let dehors = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
    let dedans = qx.max(qy).min(0.0);
    dehors + dedans - r
}

fn dist_cercle(px: f32, py: f32, cx: f32, cy: f32, r: f32) -> f32 {
    ((px - cx).powi(2) + (py - cy).powi(2)).sqrt() - r
}

fn point_dans_polygone(x: f32, y: f32, poly: &[(f32, f32)]) -> bool {
    let mut dedans = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let (xi, yi) = poly[i];
        let (xj, yj) = poly[j];
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            dedans = !dedans;
        }
        j = i;
    }
    dedans
}

fn rgb(hex: u32) -> [f32; 3] {
    [((hex >> 16) & 0xff) as f32, ((hex >> 8) & 0xff) as f32, (hex & 0xff) as f32]
}

// Peint une liste de couches (distance, couleur, opacité) l'une sur l'autre, pour une image
// de `lw` x `lh` pixels dessinée à l'échelle `echelle`.
fn peindre(lw: u32, lh: u32, echelle: f32, couches: impl Fn(f32, f32) -> Vec<(f32, [f32; 3], f32)>) -> img::Handle {
    let mut pixels = vec![0u8; (lw * lh * 4) as usize];
    for y in 0..lh {
        for x in 0..lw {
            let (px, py) = ((x as f32 + 0.5) / echelle, (y as f32 + 0.5) / echelle);
            let (mut c, mut a) = ([0.0f32; 3], 0.0f32);
            for (d, couleur, opacite) in couches(px, py) {
                let couverture = (0.5 - d * echelle).clamp(0.0, 1.0) * opacite;
                if couverture <= 0.0 { continue; }
                // mélange "par-dessus" en tenant compte de la transparence déjà peinte
                let a_nouveau = couverture + a * (1.0 - couverture);
                for k in 0..3 {
                    c[k] = (couleur[k] * couverture + c[k] * a * (1.0 - couverture)) / a_nouveau.max(1e-6);
                }
                a = a_nouveau;
            }
            let i = ((y * lw + x) * 4) as usize;
            pixels[i] = c[0] as u8;
            pixels[i + 1] = c[1] as u8;
            pixels[i + 2] = c[2] as u8;
            pixels[i + 3] = (a * 255.0) as u8;
        }
    }
    img::Handle::from_rgba(lw, lh, pixels)
}

// ---------- Icône de batterie horizontale (style "barre d'état") ----------
// Dessin : 40 x 18 unités, rendu en x3 pour être net.
fn dessiner_pile(niveau: i32, en_charge: bool) -> img::Handle {
    let e = 3.0;
    let blanc = [245.0, 245.0, 247.0];
    let remplissage = if en_charge {
        VERT.map(|v| v as f32)
    } else if niveau >= 0 && niveau <= 20 {
        ROUGE.map(|v| v as f32)
    } else {
        blanc
    };
    let largeur_pleine = 31.0 * (niveau.clamp(0, 100) as f32 / 100.0);
    let eclair: Vec<(f32, f32)> = [(19.5, 2.5), (14.0, 10.0), (18.0, 10.0), (16.5, 15.5), (22.0, 8.0), (18.0, 8.0), (19.5, 2.5)]
        .to_vec();
    peindre((40.0 * e) as u32, (18.0 * e) as u32, e, move |px, py| {
        let mut c = vec![
            // contour du corps (gris), puis on "creuse" l'intérieur avec du transparent simulé
            (dist_rect_arrondi(px, py, 1.0, 1.0, 35.0, 16.0, 5.0), [142.0, 142.0, 147.0], 1.0),
            (dist_rect_arrondi(px, py, 2.4, 2.4, 32.2, 13.2, 3.8), [28.0, 28.0, 30.0], 1.0),
            // téton à droite
            (dist_rect_arrondi(px, py, 36.6, 6.0, 2.6, 6.0, 1.3), [142.0, 142.0, 147.0], 1.0),
        ];
        if niveau >= 0 && largeur_pleine > 0.5 {
            c.push((dist_rect_arrondi(px, py, 3.5, 3.5, largeur_pleine.max(3.0), 11.0, 2.5), remplissage, 1.0));
        }
        if en_charge && point_dans_polygone(px, py, &eclair) {
            c.push((-1.0, [255.0, 255.0, 255.0], 1.0));
        }
        c
    })
}

// ---------- Boîtier ouvert (dessin original en code, vue de face légèrement plongeante) ----------
// Dessin de 200 x 170 unités, rendu x2. Dégradés + ombre au sol pour donner du volume.
const ECHELLE_BOITIER: f32 = 2.0;

fn melange(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0);
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

fn dist_ellipse(px: f32, py: f32, cx: f32, cy: f32, rx: f32, ry: f32) -> f32 {
    ((((px - cx) / rx).powi(2) + ((py - cy) / ry).powi(2)).sqrt() - 1.0) * rx.min(ry)
}

fn dessiner_boitier(avec_ecouteurs: bool) -> img::Handle {
    let e = ECHELLE_BOITIER;
    let blanc = rgb(0xFFFFFF);
    peindre((200.0 * e) as u32, (170.0 * e) as u32, e, move |px, py| {
        let mut c: Vec<(f32, [f32; 3], f32)> = Vec::new();

        // 1) Ombre douce au sol
        let k = (((px - 100.0) / 78.0).powi(2) + ((py - 160.0) / 8.0).powi(2)).sqrt();
        c.push((-1.0, [0.0, 0.0, 0.0], (1.0 - k).clamp(0.0, 1.0).powi(2) * 0.55));

        // 2) Couvercle ouvert (basculé vers l'arrière)
        let t_couv = (py - 8.0) / 58.0;
        c.push((dist_rect_arrondi(px, py, 33.0, 7.0, 134.0, 58.0, 27.0), rgb(0xC4C8CF), 1.0));
        c.push((dist_rect_arrondi(px, py, 34.0, 8.0, 132.0, 56.0, 26.0), melange(blanc, rgb(0xE2E5EA), t_couv), 1.0));
        // intérieur du couvercle, plus sombre vers la charnière
        c.push((dist_rect_arrondi(px, py, 43.0, 16.0, 114.0, 40.0, 19.0), melange(rgb(0xEEF0F3), rgb(0xC3C8D0), t_couv), 1.0));
        // charnière
        c.push((dist_rect_arrondi(px, py, 70.0, 60.0, 60.0, 8.0, 4.0), rgb(0xA9AFB8), 1.0));

        // 3) Dessus du boîtier (l'ouverture, vue d'en haut)
        c.push((dist_rect_arrondi(px, py, 24.0, 66.0, 152.0, 40.0, 20.0), rgb(0xC4C8CF), 1.0));
        c.push((dist_rect_arrondi(px, py, 25.0, 67.0, 150.0, 38.0, 19.0), melange(rgb(0xE4E7EB), rgb(0xCDD1D8), (py - 67.0) / 38.0), 1.0));
        for cx in [72.0, 128.0] {
            // logement de l'écouteur
            c.push((dist_ellipse(px, py, cx, 84.0, 21.0, 11.0), melange(rgb(0xAEB4BD), rgb(0xC6CAD1), (py - 73.0) / 22.0), 1.0));
            if avec_ecouteurs {
                // tête de l'écouteur qui dépasse : blanc avec reflet, embout gris côté intérieur
                let reflet = 1.0 - (((px - (cx - 4.0)).powi(2) + (py - 76.0).powi(2)).sqrt() / 16.0);
                c.push((dist_cercle(px, py, cx, 80.0, 13.5), rgb(0xBFC4CB), 1.0));
                c.push((dist_cercle(px, py, cx, 80.0, 12.5), melange(rgb(0xDDE0E5), blanc, reflet), 1.0));
                let interieur = if cx < 100.0 { 5.0 } else { -5.0 };
                c.push((dist_ellipse(px, py, cx + interieur, 83.0, 6.0, 4.5), rgb(0x9DA4AE), 1.0));
            }
        }

        // 4) Face avant du boîtier (cache le bas de l'ouverture : effet de profondeur)
        let t_face = (py - 92.0) / 64.0;
        c.push((dist_rect_arrondi(px, py, 19.0, 91.0, 162.0, 66.0, 31.0), rgb(0xBFC3CA), 1.0));
        c.push((dist_rect_arrondi(px, py, 20.0, 92.0, 160.0, 64.0, 30.0), melange(blanc, rgb(0xD3D7DD), t_face.powf(0.8)), 1.0));
        // reflet horizontal en haut de la face
        c.push((dist_rect_arrondi(px, py, 34.0, 96.0, 132.0, 6.0, 3.0), blanc, 0.7));

        // 5) Voyant vert avec un petit halo
        let d_led = ((px - 100.0).powi(2) + (py - 124.0).powi(2)).sqrt();
        c.push((-1.0, VERT.map(|v| v as f32), ((6.0 - d_led) / 6.0).clamp(0.0, 1.0).powi(2) * 0.35));
        c.push((dist_cercle(px, py, 100.0, 124.0, 2.6), VERT.map(|v| v as f32), 1.0));
        c
    })
}

// ---------- Cercle de batterie (comme sur l'iPhone) ----------
// Dessin de 48 x 48 unités, rendu x3 : piste grise, arc vert depuis le haut, éclair si en charge.
fn dessiner_anneau(niveau: i32, en_charge: bool) -> img::Handle {
    let e = 3.0;
    let vert = VERT.map(|v| v as f32);
    let couleur_arc = if niveau >= 0 && niveau <= 20 && !en_charge { ROUGE.map(|v| v as f32) } else { vert };
    let part = niveau.clamp(0, 100) as f32 / 100.0;
    let eclair: Vec<(f32, f32)> = [(26.0, 13.0), (17.0, 26.0), (23.0, 26.0), (21.5, 35.0), (31.0, 21.5), (25.0, 21.5), (26.0, 13.0)].to_vec();
    peindre((48.0 * e) as u32, (48.0 * e) as u32, e, move |px, py| {
        let (dx, dy) = (px - 24.0, py - 24.0);
        let d_anneau = ((dx * dx + dy * dy).sqrt() - 20.5).abs() - 2.4; // anneau de 4,8 d'épaisseur
        let mut angle = dx.atan2(-dy) / std::f32::consts::TAU;
        if angle < 0.0 { angle += 1.0; }
        let mut c = vec![(d_anneau, [58.0, 58.0, 60.0], 1.0)]; // piste
        if niveau >= 0 && angle <= part {
            c.push((d_anneau, couleur_arc, 1.0));
        }
        if en_charge && point_dans_polygone(px, py, &eclair) {
            c.push((-1.0, vert, 1.0));
        }
        c
    })
}

fn anneaux_pour(e: &Etat) -> [img::Handle; 2] {
    let (n, ch) = niveau_ecouteurs(e);
    [dessiner_anneau(n, ch), dessiner_anneau(e.boitier, e.boitier_charge)]
}

fn piles_pour(e: &Etat) -> [img::Handle; 2] {
    let (n, ch) = niveau_ecouteurs(e);
    [dessiner_pile(n, ch), dessiner_pile(e.boitier, e.boitier_charge)]
}

// ---------- Chargement de tes images ----------
// Pour ouvrir vite : la 1re fois, chaque image est réduite puis rangée dans un dossier "cache".
// Les fois suivantes, on lit directement la petite version (bien plus rapide à décoder).
// Et on charge sur plusieurs coeurs du processeur en même temps.
fn charger_une(chemin: &PathBuf, cache: &PathBuf) -> Option<image::RgbaImage> {
    let fichier_cache = cache.join(chemin.file_name()?).with_extension("png");
    let source_plus_recente = match (std::fs::metadata(chemin).and_then(|m| m.modified()),
                                     std::fs::metadata(&fichier_cache).and_then(|m| m.modified())) {
        (Ok(src), Ok(c)) => src > c,
        _ => true,
    };
    if !source_plus_recente {
        if let Ok(petite) = image::open(&fichier_cache) {
            return Some(petite.to_rgba8());
        }
    }
    let photo = image::open(chemin).map_err(|e| eprintln!("Image ignorée {}: {}", chemin.display(), e)).ok()?;
    let photo = if photo.width() > LARGEUR_MAX_IMAGE {
        photo.resize(LARGEUR_MAX_IMAGE, u32::MAX, image::imageops::FilterType::Triangle)
    } else {
        photo
    };
    let rgba = photo.to_rgba8();
    let _ = rgba.save(&fichier_cache); // si ça échoue, tant pis : on refera la prochaine fois
    Some(rgba)
}

fn charger_images(dossier: &PathBuf) -> (Vec<img::Handle>, Vec<usize>) {
    let mut chemins: Vec<PathBuf> = match std::fs::read_dir(dossier) {
        Ok(entrees) => entrees
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| {
                matches!(
                    p.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()).as_deref(),
                    Some("jpg") | Some("jpeg") | Some("png")
                )
            })
            .collect(),
        Err(e) => {
            eprintln!("Impossible de lire le dossier {}: {}", dossier.display(), e);
            return (Vec::new(), Vec::new());
        }
    };
    chemins.sort();
    let nom_source = dossier.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let cache = dossier_popup().join(format!("cache_{}_{}", LARGEUR_MAX_IMAGE, nom_source));
    let _ = std::fs::create_dir_all(&cache);

    // On découpe la liste en paquets, un par coeur du processeur
    let coeurs = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(8);
    let taille_paquet = chemins.len().div_ceil(coeurs).max(1);
    let mut resultats: Vec<Option<image::RgbaImage>> = Vec::new();
    std::thread::scope(|scope| {
        let taches: Vec<_> = chemins
            .chunks(taille_paquet)
            .map(|paquet| {
                let cache = &cache;
                scope.spawn(move || paquet.iter().map(|c| charger_une(c, cache)).collect::<Vec<_>>())
            })
            .collect();
        for t in taches {
            resultats.extend(t.join().unwrap_or_default());
        }
    });

    let (mut images, mut visibles) = (Vec::new(), Vec::new());
    for rgba in resultats.into_iter().flatten() {
        let (l, h) = rgba.dimensions();
        // nombre de pixels visibles (non transparents) : sert à trouver la partie "pose"
        visibles.push(rgba.pixels().filter(|p| p.0[3] > 10).count());
        images.push(img::Handle::from_rgba(l, h, rgba.into_raw()));
    }
    println!("{} image(s) chargée(s) depuis {}", images.len(), dossier.display());
    (images, visibles)
}

// Enlève le grand fond uni autour de l'objet (les photos Bevy font 1000x800 px) :
// on garde le plus petit rectangle qui contient tout ce qui diffère de la couleur du coin.
fn recadrer(photo: image::DynamicImage) -> image::DynamicImage {
    let rgba = photo.to_rgba8();
    let (l, h) = rgba.dimensions();
    let fond = rgba.get_pixel(0, 0).0;
    let different = |p: [u8; 4]| {
        (0..4).map(|k| (p[k] as i32 - fond[k] as i32).abs()).max().unwrap_or(0) > 12
    };
    let (mut x0, mut y0, mut x1, mut y1) = (l, h, 0, 0);
    for (x, y, p) in rgba.enumerate_pixels() {
        if different(p.0) {
            x0 = x0.min(x); y0 = y0.min(y); x1 = x1.max(x); y1 = y1.max(y);
        }
    }
    if x1 <= x0 || y1 <= y0 {
        return photo;
    }
    let marge = 12;
    let (x0, y0) = (x0.saturating_sub(marge), y0.saturating_sub(marge));
    let (x1, y1) = ((x1 + marge).min(l - 1), (y1 + marge).min(h - 1));
    photo.crop_imm(x0, y0, x1 - x0 + 1, y1 - y0 + 1)
}

// Si tu mets tes propres images du boîtier dans ~/.local/share/airpods_popup/
// (boitier_plein.png = écouteurs dedans, boitier_vide.png = sans), elles remplacent le dessin.
fn image_perso(nom: &str) -> Option<img::Handle> {
    let chemin = dossier_popup().join(nom);
    let photo = recadrer(image::open(&chemin).ok()?);
    let photo = if photo.width() > 500 { photo.resize(500, u32::MAX, image::imageops::FilterType::Triangle) } else { photo };
    let rgba = photo.to_rgba8();
    let (l, h) = rgba.dimensions();
    println!("Image du boîtier utilisée : {}", chemin.display());
    Some(img::Handle::from_rgba(l, h, rgba.into_raw()))
}

// Trouve la partie de l'animation où l'objet est bien visible (>= 90 % du maximum)
fn plage_stable(visibles: &[usize]) -> (usize, usize) {
    let max = visibles.iter().copied().max().unwrap_or(0);
    let seuil = max * 9 / 10;
    let debut = visibles.iter().position(|&v| v >= seuil).unwrap_or(0);
    let fin = visibles.iter().rposition(|&v| v >= seuil).unwrap_or(visibles.len().saturating_sub(1));
    (debut, fin.max(debut))
}

// ---------- Le programme ----------
fn boot() -> (Popup, Task<Message>) {
    let args: Vec<String> = std::env::args().collect();
    let dossier = args.get(1).map(PathBuf::from).unwrap_or_else(|| dossier_popup().join("frames"));
    let fps = args.get(2).and_then(|f| f.parse().ok()).filter(|f| *f > 0 && *f <= 120).unwrap_or(30);
    let boucle = std::env::var("POPUP_BOUCLE").map(|v| v == "1").unwrap_or(false);

    let (mut images, visibles) = if dossier.is_dir() { charger_images(&dossier) } else { (Vec::new(), Vec::new()) };
    let film_boitier = if dossier_popup().join("boitier_film").is_dir() {
        charger_images(&dossier_popup().join("boitier_film")).0
    } else {
        Vec::new()
    };
    let (mut debut_boucle, mut fin_boucle) = plage_stable(&visibles);
    let manuel = std::env::var("POPUP_MANUEL").map(|v| v == "1").unwrap_or(false);
    let charger_si = |nom: &str| {
        let d = dossier_popup().join(nom);
        if d.is_dir() { charger_images(&d).0 } else { Vec::new() }
    };
    let iph_ecouteurs = charger_si("iphone_ecouteurs");
    let iph_boitier = charger_si("iphone_boitier");
    // Pas d'images perso pour le clic sur l'icône ? On réutilise les écouteurs de la vidéo iPhone
    // (une boucle de rotation : on la joue en rond, pas en allers-retours).
    let tourne_en_rond = images.is_empty() && !iph_ecouteurs.is_empty();
    if tourne_en_rond {
        images = iph_ecouteurs.clone();
        debut_boucle = 0;
        fin_boucle = images.len() - 1;
    }
    let etat = lire_etat();
    let vide = images.is_empty();
    let popup = Popup {
        images, index: 0, debut_boucle, fin_boucle, sens: 1, fps, tourne_en_rond, boucle, manuel, ticks: 0, ticks_oreilles: 0, vu_hors_oreilles: false,
        piles: piles_pour(&etat), anneaux: anneaux_pour(&etat), etat,
        iph_ecouteurs, iph_boitier, index_iph: 0, reglages_ouverts: false, mode_en_attente: None,
        boitier_plein: image_perso("boitier_plein.png").or_else(|| image_perso("boitier.png")).unwrap_or_else(|| dessiner_boitier(true)),
        index_boitier: 0,
        boitier_ferme: film_boitier.first().cloned().or_else(|| image_perso("boitier_ferme.png"))
            .or_else(|| image_perso("boitier_vide.png"))
            .or_else(|| image_perso("boitier.png"))
            .unwrap_or_else(|| dessiner_boitier(false)),
        apparition: 0.0,
        film_boitier,
        mode: mode_demande(),
        connecte: None,
    };
    if vide {
        eprintln!("Aucune image trouvée : rien à afficher.");
        eprintln!("Lance d'abord : python3 outils/extraire_animation.py ta_video_iphone.mp4");
        return (popup, iced::exit());
    }
    (popup, Task::done(Message::VerifierConnexion))
}

fn update(p: &mut Popup, message: Message) -> Task<Message> {
    match message {
        Message::Tick => {
            p.apparition = (p.apparition + 0.06).min(1.0);
            p.ticks += 1;
            // Le film du boîtier : joué une seule fois, puis on reste sur la dernière image (couvercle ouvert)
            if p.index_boitier + 1 < p.film_boitier.len() {
                p.index_boitier += 1;
            }
            // Les animations iPhone tournent en boucle (la 1re et la dernière image se suivent)
            let n_iph = p.iph_ecouteurs.len().max(p.iph_boitier.len());
            if n_iph > 0 {
                p.index_iph = (p.index_iph + 1) % n_iph;
            }
            let secondes = p.ticks / p.fps.max(1);
            let limite_manuel = if p.mode == Mode::Bulle { DUREE_MANUEL_SECONDES } else { 60 };
            // (le compteur repart à zéro à chaque clic : la pop-up ne se ferme pas pendant que tu t'en sers)
            if secondes >= DUREE_MAX_SECONDES || (p.manuel && secondes >= limite_manuel) {
                return iced::exit();
            }
            if !p.boucle {
                // mode simple : on joue tout une fois puis on ferme
                if p.index + 1 >= p.images.len() {
                    return iced::exit();
                }
                p.index += 1;
            } else if p.tourne_en_rond {
                p.index = (p.index + 1) % p.images.len().max(1);
            } else if p.index < p.debut_boucle {
                p.index += 1; // apparition, jouée une seule fois
            } else {
                // allers-retours sur la partie "pose"
                let suivant = p.index as i32 + p.sens;
                if suivant > p.fin_boucle as i32 || suivant < p.debut_boucle as i32 {
                    p.sens = -p.sens;
                }
                p.index = (p.index as i32 + p.sens).clamp(p.debut_boucle as i32, p.fin_boucle as i32) as usize;
            }
            Task::none()
        }
        Message::RelireEtat => {
            let mut nouveau = lire_etat();
            // Une info absente (-1) ne doit pas effacer la dernière bonne valeur
            if nouveau.gauche < 0 { nouveau.gauche = p.etat.gauche; nouveau.gauche_charge = p.etat.gauche_charge; }
            if nouveau.droite < 0 { nouveau.droite = p.etat.droite; nouveau.droite_charge = p.etat.droite_charge; }
            if nouveau.boitier < 0 { nouveau.boitier = p.etat.boitier; nouveau.boitier_charge = p.etat.boitier_charge; }
            // Un mode vient d'être demandé : on l'affiche jusqu'à ce que les AirPods le confirment
            if let Some((demande, quand)) = p.mode_en_attente {
                if nouveau.ecoute == demande || quand.elapsed().as_secs() >= 4 {
                    p.mode_en_attente = None; // confirmé (ou abandon après 4 s)
                } else {
                    nouveau.ecoute = demande;
                }
            }
            if nouveau != p.etat {
                p.piles = piles_pour(&nouveau); // on ne redessine que si ça a changé
                p.anneaux = anneaux_pour(&nouveau);
                p.etat = nouveau;
            }
            // Les 2 écouteurs dans les oreilles depuis 1 s : la pop-up a fait son travail
            // (pas en mode "clic" : là, on la regarde justement avec les écouteurs sur les oreilles)
            if p.etat.en_oreille >= 2 {
                if p.vu_hors_oreilles && !p.manuel {
                    p.ticks_oreilles += 1;
                    if p.ticks_oreilles >= 2 {
                        return iced::exit();
                    }
                }
            } else {
                p.vu_hors_oreilles = true;
                p.ticks_oreilles = 0;
            }
            Task::none()
        }
        Message::Fermer => iced::exit(),
        Message::Reglages => {
            p.reglages_ouverts = !p.reglages_ouverts;
            p.ticks = 0;
            Task::none()
        }
        Message::ChoisirMode(mode) => {
            // On laisse un petit fichier "ordre" que LibrePods lit (patch 09)
            let _ = std::fs::write(dossier_popup().join("commande.txt"), format!("ecoute={}\n", mode));
            p.etat.ecoute = mode as i32; // affichage immédiat, confirmé ensuite par les AirPods
            p.mode_en_attente = Some((mode as i32, std::time::Instant::now()));
            p.ticks = 0;
            Task::none()
        }
        Message::VerifierConnexion => {
            let mac = p.etat.mac.clone();
            if mac.is_empty() {
                return Task::none();
            }
            // "bluetoothctl info" dit si l'appareil est connecté à CE PC
            Task::perform(
                async move {
                    std::process::Command::new("bluetoothctl")
                        .args(["info", &mac])
                        .output()
                        .map(|o| String::from_utf8_lossy(&o.stdout).contains("Connected: yes"))
                        .unwrap_or(false)
                },
                Message::Connexion,
            )
        }
        Message::Connexion(oui) => {
            p.connecte = Some(oui);
            Task::none()
        }
        Message::Connecter => {
            if !p.etat.mac.is_empty() {
                // On ne connecte que quand TU le demandes : pas de bagarre avec l'iPhone
                let _ = std::process::Command::new("bluetoothctl").args(["connect", &p.etat.mac]).spawn();
                p.connecte = None; // "connexion…" en attendant la prochaine vérification
            }
            Task::none()
        }
    }
}

fn texte_pourcentage(niveau: i32) -> String {
    if niveau < 0 { "—".to_string() } else { format!("{} %", niveau) }
}

fn groupe_batterie<'a>(pile: &img::Handle, texte_niveau: String, legende: &'a str) -> Element<'a, Message> {
    column![
        img(pile.clone()).width(40).height(18),
        text(texte_niveau).size(18).font(semi_gras()).color(texte_principal()),
        text(legende).size(12).color(texte_secondaire()),
    ]
    .spacing(6)
    .align_x(Alignment::Center)
    .into()
}

// Un objet animé + son cercle de batterie + le pourcentage, comme sur l'iPhone
fn objet_iphone<'a>(image: &img::Handle, anneau: &img::Handle, niveau: i32, opacite: f32) -> Element<'a, Message> {
    column![
        img(image.clone()).content_fit(ContentFit::Contain).opacity(opacite).width(210).height(190),
        img(anneau.clone()).width(46).height(46),
        text(texte_pourcentage(niveau)).size(19).color(texte_principal()),
    ]
    .spacing(8)
    .align_x(Alignment::Center)
    .width(Length::Fill)
    .into()
}

fn bouton_fermer<'a>() -> Element<'a, Message> {
    mouse_area(
        container(text("×").size(20).color(texte_secondaire()).line_height(1.0))
            .width(30)
            .height(30)
            .center(30)
            .style(|_| container::Style {
                background: Some(Color::from_rgba8(255, 255, 255, 0.10).into()),
                border: Border { radius: 15.0.into(), ..Border::default() },
                ..container::Style::default()
            }),
    )
    .on_press(Message::Fermer)
    .into()
}

fn style_carte(_: &iced::Theme) -> container::Style {
    container::Style {
        text_color: Some(texte_principal()),
        background: Some(fond_carte().into()),
        border: Border { color: bord_carte(), width: 1.0, radius: 32.0.into() },
        shadow: Shadow { color: Color::from_rgba8(0, 0, 0, 0.45), offset: Vector::new(0.0, 14.0), blur_radius: 34.0 },
        snap: false,
    }
}

fn titre<'a>(nom: &str) -> Element<'a, Message> {
    text(nom.to_string())
        .size(24)
        .font(gras())
        .color(texte_principal())
        .width(Length::Fill)
        .align_x(Alignment::Center)
        .into()
}

fn view_iphone(p: &Popup) -> Element<'_, Message> {
    let opacite = p.apparition;
    let (n_ecouteurs, _) = niveau_ecouteurs(&p.etat);
    let i_e = p.index_iph % p.iph_ecouteurs.len().max(1);
    let i_b = p.index_iph % p.iph_boitier.len().max(1);
    let objets = row![
        objet_iphone(&p.iph_ecouteurs[i_e], &p.anneaux[0], n_ecouteurs, opacite),
        objet_iphone(&p.iph_boitier[i_b], &p.anneaux[1], p.etat.boitier, opacite),
    ];
    let mut contenu = column![
        row![iced::widget::Space::new().width(30), titre(&p.etat.nom), bouton_fermer()].align_y(Alignment::Center),
        iced::widget::Space::new().height(10),
        objets,
    ]
    .spacing(12);
    // Le bouton "Connecter" n'apparaît que si les AirPods ne sont pas connectés à ce PC
    if p.connecte == Some(false) {
        let bouton = container(text("Connecter").size(16).font(semi_gras()).color(texte_principal()))
            .width(Length::Fill)
            .height(46)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .style(|_| container::Style {
                background: Some(Color::from_rgb8(0x0A, 0x84, 0xFF).into()),
                border: Border { radius: 14.0.into(), ..Border::default() },
                ..container::Style::default()
            });
        contenu = contenu.push(mouse_area(bouton).on_press(Message::Connecter));
    }
    let carte = container(contenu)
        .padding(Padding { top: 24.0, right: 22.0, bottom: 28.0, left: 22.0 })
        .width(LARGEUR_CARTE)
        .style(style_carte);
    container(carte).width(Length::Fill).height(Length::Fill).center(Length::Fill).into()
}

fn view_carte(p: &Popup) -> Element<'_, Message> {
    // Si les images de ta vidéo iPhone sont là, on fait la pop-up façon iPhone
    if !p.iph_ecouteurs.is_empty() && !p.iph_boitier.is_empty() {
        return view_iphone(p);
    }
    // --- En-tête : nom centré, bouton rond pour fermer
    let titre = text(p.etat.nom.clone())
        .size(22)
        .font(gras())
        .color(texte_principal())
        .width(Length::Fill)
        .align_x(Alignment::Center);
    let fermer = mouse_area(
        container(text("×").size(20).color(texte_secondaire()).line_height(1.0))
            .width(30)
            .height(30)
            .center(30)
            .style(|_| container::Style {
                background: Some(Color::from_rgba8(255, 255, 255, 0.10).into()),
                border: Border { radius: 15.0.into(), ..Border::default() },
                ..container::Style::default()
            }),
    )
    .on_press(Message::Fermer);
    let entete = row![iced::widget::Space::new().width(30), titre, fermer].align_y(Alignment::Center);

    // --- Les écouteurs : tes images (chaque image contient déjà la paire)
    let opacite = p.apparition;
    let paire = img(p.images[p.index.min(p.images.len().max(1) - 1)].clone())
        .content_fit(ContentFit::Contain)
        .opacity(opacite)
        .width(300)
        .height(200);

    // --- Le scénario :
    //  * écouteurs dans le boîtier -> seulement le boîtier ouvert, écouteurs dedans (en grand)
    //  * écouteurs sortis          -> l'animation de tes écouteurs + le boîtier vide à côté
    let dans_boitier = p.etat.dans_boitier >= 2;
    let visuel: Element<'_, Message> = if dans_boitier {
        let image_boitier = p.film_boitier.get(p.index_boitier).cloned().unwrap_or_else(|| p.boitier_plein.clone());
        container(img(image_boitier).content_fit(ContentFit::Contain).opacity(opacite).width(360).height(210))
            .width(Length::Fill)
            .height(210)
            .center(Length::Fill)
            .into()
    } else {
        // boîtier fermé à gauche, tes écouteurs animés à droite
        row![
            container(img(p.boitier_ferme.clone()).content_fit(ContentFit::Contain).opacity(opacite).width(190).height(162))
                .width(Length::FillPortion(2))
                .height(210)
                .center(Length::Fill),
            container(paire).width(Length::FillPortion(3)).center_x(Length::Fill),
        ]
        .align_y(Alignment::Center)
        .into()
    };

    // --- Batteries, chacune sous son objet
    let (n_ecouteurs, _) = niveau_ecouteurs(&p.etat);
    let texte_ecouteurs = if p.etat.gauche >= 0 && p.etat.droite >= 0 && (p.etat.gauche - p.etat.droite).abs() > 5 {
        format!("G {} · D {}", texte_pourcentage(p.etat.gauche), texte_pourcentage(p.etat.droite))
    } else {
        texte_pourcentage(n_ecouteurs)
    };
    // Batteries dans le même ordre que les objets : boîtier à gauche, écouteurs à droite
    let (part_boitier, part_ecouteurs) = if dans_boitier { (1, 1) } else { (2, 3) };
    let batteries = row![
        container(groupe_batterie(&p.piles[1], texte_pourcentage(p.etat.boitier), "Boîtier"))
            .width(Length::FillPortion(part_boitier))
            .center_x(Length::Fill),
        container(groupe_batterie(&p.piles[0], texte_ecouteurs, "Écouteurs"))
            .width(Length::FillPortion(part_ecouteurs))
            .center_x(Length::Fill),
    ];

    // Bouton "Connecter" (comme sur iPhone) : on se connecte seulement si tu le demandes
    let (texte_bouton, couleur_bouton, action) = match p.connecte {
        Some(true) => ("Connectés à ce PC", Color::from_rgba8(255, 255, 255, 0.10), None),
        Some(false) => ("Connecter", Color::from_rgb8(0x0A, 0x84, 0xFF), Some(Message::Connecter)),
        None => ("Connexion…", Color::from_rgba8(255, 255, 255, 0.10), None),
    };
    let bouton = container(text(texte_bouton).size(15).font(semi_gras()).color(texte_principal()))
        .width(Length::Fill)
        .height(44)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(move |_| container::Style {
            background: Some(couleur_bouton.into()),
            border: Border { radius: 14.0.into(), ..Border::default() },
            ..container::Style::default()
        });
    let bouton: Element<'_, Message> = match action {
        Some(m) => mouse_area(bouton).on_press(m).into(),
        None => bouton.into(),
    };

    let carte = container(column![entete, visuel, batteries, bouton].spacing(12))
        .padding(Padding { top: 22.0, right: 22.0, bottom: 26.0, left: 22.0 })
        .width(LARGEUR_CARTE)
        .style(|_| container::Style {
            text_color: Some(texte_principal()),
            background: Some(fond_carte().into()),
            border: Border { color: bord_carte(), width: 1.0, radius: 32.0.into() },
            shadow: Shadow { color: Color::from_rgba8(0, 0, 0, 0.45), offset: Vector::new(0.0, 14.0), blur_radius: 34.0 },
            snap: false,
        });

    container(carte).width(Length::Fill).height(Length::Fill).center(Length::Fill).into()
}

// ---------- Clic sur l'icône : tes écouteurs (146 images) + batterie + réglages ----------
fn bouton_mode<'a>(libelle: &'a str, valeur: u8, actif: bool) -> Element<'a, Message> {
    let (fond, texte_c) = if actif {
        (Color::from_rgb8(0xF5, 0xF5, 0xF7), Color::from_rgb8(0x1C, 0x1C, 0x1E))
    } else {
        (Color::from_rgba8(255, 255, 255, 0.08), texte_principal())
    };
    mouse_area(
        container(text(libelle).size(13).font(semi_gras()).color(texte_c).align_x(Alignment::Center))
            .width(Length::Fill)
            .height(48)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .style(move |_| container::Style {
                background: Some(fond.into()),
                border: Border { radius: 12.0.into(), ..Border::default() },
                ..container::Style::default()
            }),
    )
    .on_press(Message::ChoisirMode(valeur))
    .into()
}

fn view_bulle(p: &Popup) -> Element<'_, Message> {
    let opacite = p.apparition;
    let ecouteurs = img(p.images[p.index.min(p.images.len().max(1) - 1)].clone())
        .content_fit(ContentFit::Contain)
        .opacity(opacite)
        .width(Length::Fill)
        .height(190);
    let (n_ecouteurs, _) = niveau_ecouteurs(&p.etat);
    let batterie = |anneau: &img::Handle, niveau: i32, nom: &'static str| -> Element<'_, Message> {
        row![
            img(anneau.clone()).width(34).height(34),
            column![
                text(texte_pourcentage(niveau)).size(16).font(semi_gras()).color(texte_principal()),
                text(nom).size(12).color(texte_secondaire()),
            ]
            .spacing(1),
        ]
        .spacing(10)
        .align_y(Alignment::Center)
        .into()
    };
    let batteries = row![
        container(batterie(&p.anneaux[0], n_ecouteurs, "Écouteurs")).width(Length::Fill).center_x(Length::Fill),
        container(batterie(&p.anneaux[1], p.etat.boitier, "Boîtier")).width(Length::Fill).center_x(Length::Fill),
    ];

    // Bouton "Réglages" -> fait apparaître les modes d'écoute
    let libelle_reglages = if p.reglages_ouverts { "Masquer les réglages" } else { "Réglages du son" };
    let bouton_reglages = mouse_area(
        container(text(libelle_reglages).size(15).font(semi_gras()).color(texte_principal()))
            .width(Length::Fill)
            .height(44)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .style(|_| container::Style {
                background: Some(Color::from_rgba8(255, 255, 255, 0.10).into()),
                border: Border { radius: 14.0.into(), ..Border::default() },
                ..container::Style::default()
            }),
    )
    .on_press(Message::Reglages);

    let mut contenu = column![
        row![iced::widget::Space::new().width(30), titre(&p.etat.nom), bouton_fermer()].align_y(Alignment::Center),
        ecouteurs,
        batteries,
        bouton_reglages,
    ]
    .spacing(14);
    if p.reglages_ouverts {
        let e = p.etat.ecoute;
        contenu = contenu.push(
            row![
                bouton_mode("Réduction\ndu bruit", 2, e == 2),
                bouton_mode("Adaptatif", 4, e == 4),
                bouton_mode("Transparence", 3, e == 3),
                bouton_mode("Désactivé", 1, e == 1),
            ]
            .spacing(6),
        );
    }
    let carte = container(contenu)
        .padding(Padding { top: 22.0, right: 20.0, bottom: 22.0, left: 20.0 })
        .width(LARGEUR_CARTE - 40.0)
        .style(style_carte);
    container(carte).width(Length::Fill).height(Length::Fill).center_x(Length::Fill).align_y(Alignment::Start).padding(10).into()
}

fn view(p: &Popup) -> Element<'_, Message> {
    match p.mode {
        Mode::Bulle => view_bulle(p),
        Mode::Carte => view_carte(p),
    }
}

fn subscription(p: &Popup) -> Subscription<Message> {
    Subscription::batch([
        iced::time::every(Duration::from_millis(1000 / p.fps)).map(|_| Message::Tick),
        iced::time::every(Duration::from_millis(500)).map(|_| Message::RelireEtat),
        iced::time::every(Duration::from_millis(2000)).map(|_| Message::VerifierConnexion),
    ])
}

fn main() -> iced::Result {
    let bulle = mode_demande() == Mode::Bulle;
    iced::application(boot, update, view)
        .title("AirPods")
        .subscription(subscription)
        .style(|_, _| iced::theme::Style { background_color: Color::TRANSPARENT, text_color: Color::WHITE })
        .window(window::Settings {
            size: if bulle { Size::new(LARGEUR_BULLE, HAUTEUR_BULLE) } else { Size::new(LARGEUR_FENETRE, HAUTEUR_FENETRE) },
            // (la fenêtre "clic" est placée en haut à droite, sous l'icône)
            position: if bulle {
                // en haut à droite, juste sous la barre du haut
                window::Position::SpecificWith(|fenetre, ecran| iced::Point::new(ecran.width - fenetre.width - 8.0, 30.0))
            } else {
                window::Position::Centered
            },
            decorations: false,
            transparent: true,
            resizable: false,
            level: window::Level::AlwaysOnTop,
            ..window::Settings::default()
        })
        .run()
}
