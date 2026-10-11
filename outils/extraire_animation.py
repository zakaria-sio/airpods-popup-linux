#!/usr/bin/env python3
"""
extraire_animation.py
=====================

Fabrique l'animation de la pop-up (écouteurs + boîtier qui tournent)
à partir d'un ENREGISTREMENT D'ÉCRAN DE TON IPHONE.

Pourquoi un script ? Les images viennent d'Apple : on n'a pas le droit de les
mettre sur GitHub. Ce script fait le travail chez toi, avec ta propre vidéo.

Ce qu'il fait tout seul :
  1. trouve la carte sombre de la pop-up dans la vidéo ;
  2. trouve les écouteurs (à gauche) et le boîtier (à droite) ;
  3. enlève le fond (les images deviennent transparentes) ;
  4. trouve la boucle de rotation (pour que l'animation tourne sans saut) ;
  5. range les images là où la pop-up les cherche.

Utilisation :
    python3 extraire_animation.py ma_video.mp4

Options utiles :
    --sortie DOSSIER   où ranger les images (par défaut ~/.local/share/airpods_popup)
    --debut 3 --fin 20 n'analyser qu'un morceau de la vidéo (en secondes)

Il faut : ffmpeg, python3-numpy, python3-pil
    sudo apt install ffmpeg python3-numpy python3-pil

Licence : AGPL-3.0 (comme LibrePods).
"""

import argparse
import shutil
import subprocess
import sys
from io import BytesIO
from pathlib import Path

try:
    import numpy as np
    from PIL import Image, ImageDraw
except ImportError:
    sys.exit("Il manque numpy ou Pillow. Installe-les avec :\n"
             "    sudo apt install python3-numpy python3-pil")

IPS = 30                 # images par seconde (la pop-up joue à 30 images/s)
LARGEUR_ANALYSE = 390    # on analyse une version réduite de la vidéo (plus rapide)
LARGEUR_MAX_OBJET = 420  # taille max d'une image finale (la pop-up l'affiche en ~210 px)
SEUIL_FOND = 10          # écart max (0-255) pour dire "ce pixel est le fond de la carte"
SEUIL_OBJET = 30         # écart min pour dire "ce pixel fait partie d'un objet"


def dire(texte):
    print(texte, flush=True)


def arreter(texte):
    sys.exit("\n❌ " + texte)


# ---------------------------------------------------------------- vidéo ----

def verifier_outils():
    for outil in ("ffmpeg", "ffprobe"):
        if shutil.which(outil) is None:
            arreter(f"{outil} n'est pas installé. Fais : sudo apt install ffmpeg")


def options_morceau(debut, fin):
    opts = []
    if debut is not None:
        opts += ["-ss", str(debut)]
    if fin is not None:
        opts += ["-to", str(fin)]
    return opts


def taille_video(chemin):
    """Taille réelle de l'image (après rotation éventuelle), lue sur la 1re image."""
    r = subprocess.run(
        ["ffmpeg", "-loglevel", "error", "-i", str(chemin), "-frames:v", "1",
         "-f", "image2pipe", "-vcodec", "png", "-"],
        capture_output=True)
    if r.returncode != 0 or not r.stdout:
        arreter(f"Impossible de lire la vidéo : {chemin}\n{r.stderr.decode(errors='ignore')}")
    return Image.open(BytesIO(r.stdout)).size


def lire_images(chemin, largeur, hauteur, debut, fin):
    """Lit la vidéo image par image (à 30 images/s), sans tout charger en mémoire."""
    cmd = (["ffmpeg", "-loglevel", "error"] + options_morceau(debut, fin) +
           ["-i", str(chemin),
            "-vf", f"fps={IPS},scale={largeur}:{hauteur}:flags=area",
            "-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
    taille = largeur * hauteur * 3
    proc = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    lues = 0
    try:
        while True:
            brut = proc.stdout.read(taille)
            if len(brut) < taille:
                break
            lues += 1
            yield np.frombuffer(brut, np.uint8).reshape(hauteur, largeur, 3)
    finally:
        fini = proc.poll() is not None
        if not fini:
            proc.terminate()   # on a eu ce qu'on voulait : on arrête ffmpeg sans bruit
        proc.stdout.close()
        erreurs = proc.stderr.read().decode(errors="ignore")
        proc.wait()
        if fini and lues == 0:
            arreter("ffmpeg n'a pas réussi à lire la vidéo :\n" + erreurs)


def hauteur_pour(largeur, l_video, h_video):
    return max(2, int(round(h_video * largeur / l_video / 2)) * 2)


# --------------------------------------------------- trouver la carte ----

def ecart(img, couleur):
    """Pour chaque pixel : la plus grande différence (sur R, V, B) avec la couleur donnée."""
    return np.abs(img.astype(np.int16) - np.asarray(couleur, np.int16)).max(axis=2)


def trouver_carte(img):
    """Renvoie (haut, bas, gauche, droite, couleur_fond) de la carte, ou None."""
    h = img.shape[0]
    bas_ecran = img[h // 3::3, ::3].reshape(-1, 3)  # la carte est en bas de l'écran
    arrondi = bas_ecran // 4
    couleurs, nb = np.unique(arrondi, axis=0, return_counts=True)
    # la couleur la plus présente = le fond de la carte (on prend la moyenne exacte de ces pixels)
    fond = bas_ecran[(arrondi == couleurs[nb.argmax()]).all(axis=1)].mean(axis=0).round()
    masque = ecart(img, fond) <= SEUIL_FOND
    lignes = np.where(masque.mean(axis=1) > 0.6)[0]
    if len(lignes) < 20:
        return None
    haut, bas = lignes.min(), lignes.max()
    colonnes = np.where(masque[haut:bas + 1].mean(axis=0) > 0.5)[0]
    if len(colonnes) < 20:
        return None
    return int(haut), int(bas), int(colonnes.min()), int(colonnes.max()), tuple(int(c) for c in fond)


def groupes(profil, trou_max):
    """Regroupe les positions où profil > 0 en morceaux (début, fin), en ignorant les petits trous."""
    pos = np.where(profil > 0)[0]
    if len(pos) == 0:
        return []
    morceaux, debut, prec = [], pos[0], pos[0]
    for p in pos[1:]:
        if p - prec > trou_max + 1:
            morceaux.append((int(debut), int(prec)))
            debut = p
        prec = p
    morceaux.append((int(debut), int(prec)))
    return morceaux


def trouver_objets(images, carte):
    """Trouve les 2 rectangles (écouteurs, boîtier) qui contiennent les objets sur toutes les images."""
    haut, bas, gauche, droite, fond = carte
    marge = 4
    union = None
    for img in images:
        zone = img[haut + marge:bas - marge, gauche + marge:droite - marge]
        objet = ecart(zone, fond) > SEUIL_OBJET
        union = objet if union is None else (union | objet)

    # De haut en bas, la carte contient : le titre, les objets, les cercles, les %.
    # Les objets = la bande la plus haute.
    bandes = groupes(union.sum(axis=1) > 1, trou_max=3)
    if not bandes:
        return None
    y0, y1 = max(bandes, key=lambda b: b[1] - b[0])

    # Dans cette bande : 2 groupes côte à côte (écouteurs à gauche, boîtier à droite)
    morceaux = groupes(union[y0:y1 + 1].sum(axis=0) > 0, trou_max=6)
    if len(morceaux) < 2:
        return None
    # s'il y en a plus de 2, on coupe au plus grand espace vide
    trous = [morceaux[i + 1][0] - morceaux[i][1] for i in range(len(morceaux) - 1)]
    coupe = int(np.argmax(trous))
    g = (morceaux[0][0], morceaux[coupe][1])
    d = (morceaux[coupe + 1][0], morceaux[-1][1])

    def rect(x0, x1):
        bande = union[y0:y1 + 1, x0:x1 + 1]
        ys = np.where(bande.any(axis=1))[0]
        m = 3
        return (gauche + marge + x0 - m, haut + marge + y0 + ys.min() - m,
                gauche + marge + x1 + m + 1, haut + marge + y0 + ys.max() + m + 1)

    return rect(*g), rect(*d)


# ------------------------------------------------- trouver la boucle ----

def trouver_boucle(sequence, nom):
    """sequence : tableau (nb_images, pixels). Renvoie (début, longueur) de la boucle."""
    n = len(sequence)
    if sequence.std(axis=0).mean() < 1.0:
        dire(f"   {nom} : l'objet ne bouge pas, une seule image suffit.")
        return 0, 1
    mini, maxi = int(1.0 * IPS), min(int(15 * IPS), n - IPS // 2)
    if maxi <= mini:
        dire(f"   {nom} : vidéo trop courte pour trouver une boucle, on garde tout.")
        return 0, n
    scores = np.array([np.abs(sequence[:-L] - sequence[L:]).mean() for L in range(mini, maxi)])
    meilleur = scores.min()
    longueur = None
    for i in range(1, len(scores) - 1):          # la plus petite boucle presque aussi bonne que la meilleure
        if scores[i] <= scores[i - 1] and scores[i] <= scores[i + 1] and scores[i] <= meilleur * 1.25 + 0.5:
            longueur = mini + i
            break
    if longueur is None:
        longueur = mini + int(scores.argmin())
    # Le point de départ où le "raccord" fin -> début est le plus invisible
    raccords = np.abs(sequence[:n - longueur] - sequence[longueur:]).mean(axis=1)
    debut = int(raccords.argmin())
    qualite = raccords[debut]
    dire(f"   {nom} : boucle de {longueur} images ({longueur / IPS:.1f} s), raccord {qualite:.1f}"
         + ("  ✓" if qualite < 6 else "  ⚠ raccord visible, essaie une vidéo plus longue"))
    return debut, longueur


# ------------------------------------------------------ détourage ----

def detourer(img, fond):
    """Rend le fond transparent. Posée sur le fond de la carte, l'image redonne exactement l'original."""
    f = np.asarray(fond, np.float32)
    px = img.astype(np.float32)
    d = np.abs(px - f).max(axis=2)
    alpha = np.clip((d - 8.0) / 32.0, 0.0, 1.0)
    a = np.maximum(alpha, 1e-3)[..., None]
    couleur = np.clip(f + (px - f) / a, 0, 255)
    rgba = np.dstack([couleur, alpha * 255]).round().astype(np.uint8)
    return Image.fromarray(rgba, "RGBA")


def poser_sur_toile(image, toile, facteur):
    """Centre l'objet sur une toile transparente de taille commune, puis réduit.
    Les 2 objets ont ainsi la même taille de toile : la pop-up garde leurs proportions."""
    fond = Image.new("RGBA", toile, (0, 0, 0, 0))
    fond.paste(image, ((toile[0] - image.width) // 2, (toile[1] - image.height) // 2))
    if facteur >= 1:
        return fond
    taille = (max(1, round(toile[0] * facteur)), max(1, round(toile[1] * facteur)))
    # "RGBa" = couleurs pré-multipliées : évite un liseré sombre sur les bords
    return fond.convert("RGBa").resize(taille, Image.LANCZOS).convert("RGBA")


# ---------------------------------------------------------- aperçu ----

def faire_apercu(reference, carte, rects, dossiers, fond, chemin):
    ref = Image.fromarray(reference).convert("RGB")
    dessin = ImageDraw.Draw(ref)
    haut, bas, gauche, droite, _ = carte
    dessin.rectangle([gauche, haut, droite, bas], outline=(255, 200, 0), width=2)
    for r, c in zip(rects, [(0, 200, 255), (255, 60, 200)]):
        dessin.rectangle(r, outline=c, width=2)
    lignes = []
    for d in dossiers:
        fichiers = sorted(d.glob("*.png"))
        choix = [fichiers[i * len(fichiers) // 6] for i in range(min(6, len(fichiers)))]
        vignettes = []
        for f in choix:
            im = Image.open(f)
            im.thumbnail((140, 140))
            case = Image.new("RGB", (150, 150), tuple(fond))
            case.paste(im, ((150 - im.width) // 2, (150 - im.height) // 2), im)
            vignettes.append(case)
        ligne = Image.new("RGB", (150 * 6, 150), tuple(fond))
        for i, v in enumerate(vignettes):
            ligne.paste(v, (i * 150, 0))
        lignes.append(ligne)
    ref.thumbnail((300, 650))
    planche = Image.new("RGB", (ref.width + 20 + 900, max(ref.height, 320)), (40, 40, 44))
    planche.paste(ref, (0, 0))
    for i, l in enumerate(lignes):
        planche.paste(l, (ref.width + 20, i * 160))
    planche.save(chemin)


# ------------------------------------------------------------ main ----

def main():
    p = argparse.ArgumentParser(description="Découpe l'animation de la pop-up AirPods dans un enregistrement d'écran iPhone.")
    p.add_argument("video", type=Path, help="l'enregistrement d'écran (.mp4, .mov…)")
    p.add_argument("--sortie", type=Path, default=Path.home() / ".local/share/airpods_popup",
                   help="dossier de la pop-up (par défaut ~/.local/share/airpods_popup)")
    p.add_argument("--debut", type=float, help="ignorer la vidéo avant ce moment (secondes)")
    p.add_argument("--fin", type=float, help="ignorer la vidéo après ce moment (secondes)")
    a = p.parse_args()

    verifier_outils()
    if not a.video.is_file():
        arreter(f"Fichier introuvable : {a.video}")

    l_video, h_video = taille_video(a.video)
    dire(f"🎬 Vidéo : {l_video}×{h_video}")

    # ---- 1er passage : version réduite, pour tout analyser
    dire("🔎 Analyse de la vidéo…")
    la = min(LARGEUR_ANALYSE, l_video)
    ha = hauteur_pour(la, l_video, h_video)
    petites, couleurs = [], []
    for i, img in enumerate(lire_images(a.video, la, ha, a.debut, a.fin)):
        petites.append(img[::2, ::2].mean(axis=2).astype(np.float32))  # en gris, 2× plus petit
        if i % 5 == 0:
            couleurs.append((i, img.copy()))
    n = len(petites)
    if n < IPS:
        arreter("La vidéo est trop courte (il faut au moins quelques secondes de pop-up).")
    dire(f"   {n} images ({n / IPS:.1f} s)")

    # ---- la carte : la position la plus fréquente
    cartes = [(i, img, trouver_carte(img)) for i, img in couleurs]
    trouvees = [c for c in cartes if c[2] is not None]
    if not trouvees:
        arreter("Je ne trouve pas la carte de la pop-up. Vérifie que la vidéo montre bien la pop-up AirPods.")
    cles = [c[2][:4] for c in trouvees]
    plus_frequente = max(set(cles), key=lambda k: sum(1 for x in cles if all(abs(x[j] - k[j]) <= 2 for j in range(4))))
    stables = [c for c in trouvees if all(abs(c[2][j] - plus_frequente[j]) <= 2 for j in range(4))]
    carte = stables[len(stables) // 2][2]
    reference = stables[len(stables) // 2][1]
    dire(f"   Carte trouvée (fond = RVB{carte[4]})")

    rects = trouver_objets([c[1] for c in stables], carte)
    if rects is None:
        arreter("Je trouve la carte, mais pas les deux objets dedans.")
    dire("   Écouteurs et boîtier trouvés")

    # ---- les moments où la carte est bien posée (pas en train d'apparaître / partir)
    haut, bas, gauche, droite, fond = carte
    ref_gris = reference[::2, ::2].mean(axis=2).astype(np.float32)
    masque = np.zeros_like(ref_gris, bool)
    masque[haut // 2:bas // 2, gauche // 2:droite // 2] = True
    for x0, y0, x1, y1 in rects:
        masque[y0 // 2:y1 // 2 + 1, x0 // 2:x1 // 2 + 1] = False
    ok = np.array([np.abs(g[masque] - ref_gris[masque]).mean() < 6 for g in petites])
    morceaux = groupes(ok, trou_max=2)
    debut_ok, fin_ok = max(morceaux, key=lambda m: m[1] - m[0])
    dire(f"   Pop-up immobile de {debut_ok / IPS:.1f} s à {fin_ok / IPS:.1f} s")

    # ---- la boucle de rotation de chaque objet
    dire("🔁 Recherche de la boucle de rotation…")
    choix = []
    for (x0, y0, x1, y1), nom in zip(rects, ["Écouteurs", "Boîtier"]):
        seq = np.stack([g[y0 // 2:y1 // 2 + 1, x0 // 2:x1 // 2 + 1].ravel()
                        for g in petites[debut_ok:fin_ok + 1]])
        d, longueur = trouver_boucle(seq, nom)
        choix.append(range(debut_ok + d, debut_ok + d + longueur))
    del petites

    # ---- 2e passage : pleine qualité, seulement les images utiles
    dire("✂️  Découpage en pleine qualité…")
    dossiers = [a.sortie / "iphone_ecouteurs", a.sortie / "iphone_boitier"]
    for d in dossiers:
        d.mkdir(parents=True, exist_ok=True)
        for ancien in d.glob("*.png"):
            ancien.unlink()
        cache = a.sortie / f"cache_600_{d.name}"
        if cache.is_dir():
            shutil.rmtree(cache)
    echelle = l_video / la
    boites = []
    for x0, y0, x1, y1 in rects:
        X0, Y0 = max(0, int(x0 * echelle)), max(0, int(y0 * echelle))
        X1, Y1 = min(l_video, int(x1 * echelle) + 1), min(h_video, int(y1 * echelle) + 1)
        boites.append((X0, Y0, X1, Y1))
    toile = (max(b[2] - b[0] for b in boites), max(b[3] - b[1] for b in boites))
    facteur = min(1.0, LARGEUR_MAX_OBJET / toile[0])
    voulues = set(choix[0]) | set(choix[1])
    derniere = max(voulues)
    for i, img in enumerate(lire_images(a.video, l_video, hauteur_pour(l_video, l_video, h_video), a.debut, a.fin)):
        if i > derniere:
            break
        if i not in voulues:
            continue
        for (X0, Y0, X1, Y1), plage, dossier in zip(boites, choix, dossiers):
            if i in plage:
                image = poser_sur_toile(detourer(img[Y0:Y1, X0:X1], fond), toile, facteur)
                image.save(dossier / f"{i - plage.start + 1:04d}.png", optimize=False)

    apercu = a.sortie / "apercu_decoupage.png"
    faire_apercu(reference, carte, rects, dossiers, fond, apercu)

    dire("\n✅ Terminé !")
    for d in dossiers:
        dire(f"   {len(list(d.glob('*.png')))} images dans {d}")
    dire(f"   Aperçu pour vérifier : {apercu}")
    dire("   Ouvre la pop-up (clic sur l'icône LibrePods) pour voir le résultat.")


if __name__ == "__main__":
    main()
