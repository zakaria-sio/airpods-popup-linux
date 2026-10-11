#!/usr/bin/env bash
# Installe LibrePods + la pop-up AirPods façon iPhone.
# Utilisation :  ./install.sh
# Licence : AGPL-3.0
set -euo pipefail

DEPOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LIBREPODS_URL="https://github.com/kavishdevar/librepods.git"
LIBREPODS_BRANCHE="linux/rust"
LIBREPODS_COMMIT="672e65a"     # la version sur laquelle les patchs ont été testés
SOURCES="${LIBREPODS_DIR:-$HOME/.local/share/librepods-popup/librepods}"
BIN="$HOME/.local/bin"
DONNEES="$HOME/.local/share/airpods_popup"

vert()  { printf '\033[32m%s\033[0m\n' "$*"; }
jaune() { printf '\033[33m%s\033[0m\n' "$*"; }
rouge() { printf '\033[31m%s\033[0m\n' "$*"; }
etape() { printf '\n\033[1m==> %s\033[0m\n' "$*"; }
oui()   { read -r -p "$1 [o/N] " r; [[ "$r" =~ ^[oOyY]$ ]]; }

# ---------------------------------------------------------------------------
etape "1/6 Vérification des outils"
manque=()
for paquet in git pkg-config build-essential libdbus-1-dev libpulse-dev ffmpeg; do
    dpkg -s "$paquet" >/dev/null 2>&1 || manque+=("$paquet")
done
# numpy et Pillow peuvent aussi venir de pip : on teste directement dans Python
python3 -c "import numpy" 2>/dev/null || manque+=(python3-numpy)
python3 -c "import PIL" 2>/dev/null || manque+=(python3-pil)
if ((${#manque[@]})); then
    jaune "Il manque : ${manque[*]}"
    if oui "Je les installe avec sudo apt ?"; then
        sudo apt update && sudo apt install -y "${manque[@]}"
    else
        rouge "Installe-les puis relance : bash install.sh"; exit 1
    fi
fi
if ! command -v cargo >/dev/null; then
    [ -f "$HOME/.cargo/env" ] && source "$HOME/.cargo/env"
fi
if ! command -v cargo >/dev/null; then
    rouge "Rust (cargo) n'est pas installé."
    echo "Installe-le avec :  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    echo "puis ferme et rouvre le terminal, et relance : bash install.sh"
    exit 1
fi
vert "OK"

# ---------------------------------------------------------------------------
etape "2/6 Téléchargement de LibrePods ($LIBREPODS_COMMIT)"
if [ -d "$SOURCES/.git" ]; then
    echo "Déjà téléchargé dans $SOURCES"
else
    mkdir -p "$(dirname "$SOURCES")"
    git clone --branch "$LIBREPODS_BRANCHE" "$LIBREPODS_URL" "$SOURCES"
fi
cd "$SOURCES"
if [ -n "$(git status --porcelain --untracked-files=no)" ]; then
    jaune "Des fichiers ont déjà été modifiés dans $SOURCES."
    if oui "Je remets LibrePods à zéro (tes modifs dans ce dossier seront perdues) ?"; then
        git checkout -- .
    else
        rouge "Arrêt."; exit 1
    fi
fi
git -c advice.detachedHead=false checkout -q "$LIBREPODS_COMMIT"
vert "OK"

# ---------------------------------------------------------------------------
etape "3/6 Application des patchs"
for patch in "$DEPOT"/patches/*.patch; do
    if git apply --check "$patch" 2>/dev/null; then
        git apply "$patch"
        echo "  ✓ $(basename "$patch")"
    else
        rouge "  ✗ $(basename "$patch") ne s'applique pas."; exit 1
    fi
done

# ---------------------------------------------------------------------------
etape "4/6 Compilation (ça peut prendre 10 à 20 minutes la 1re fois)"
(cd "$SOURCES/linux-rust" && cargo build --release)
(cd "$DEPOT/popup" && cargo build --release)
mkdir -p "$BIN" "$DONNEES"
install -m 755 "$SOURCES/linux-rust/target/release/librepods" "$BIN/librepods"
install -m 755 "$DEPOT/popup/target/release/airpods-popup" "$BIN/airpods-popup"
vert "Installé : $BIN/librepods et $BIN/airpods-popup"

# ---------------------------------------------------------------------------
etape "5/6 Bluetooth : mode expérimental de BlueZ"
# LibrePods en a besoin pour écouter les signaux BLE (couvercle ouvert/fermé)
CONF=/etc/bluetooth/main.conf
if grep -Eq '^[[:space:]]*Experimental[[:space:]]*=[[:space:]]*true' "$CONF" 2>/dev/null; then
    vert "Déjà activé"
else
    jaune "Il faut mettre 'Experimental = true' dans $CONF (une sauvegarde .bak sera faite)."
    if oui "Je le fais avec sudo et je redémarre le Bluetooth ?"; then
        sudo cp "$CONF" "$CONF.bak"
        if grep -Eq '^[[:space:]]*#?[[:space:]]*Experimental[[:space:]]*=' "$CONF"; then
            sudo sed -i -E 's/^[[:space:]]*#?[[:space:]]*Experimental[[:space:]]*=.*/Experimental = true/' "$CONF"
        else
            sudo sed -i -E 's/^\[General\]/[General]\nExperimental = true/' "$CONF"
        fi
        sudo systemctl restart bluetooth
        vert "Activé"
    else
        jaune "Pas fait : la pop-up ne s'ouvrira pas toute seule à l'ouverture du boîtier."
    fi
fi

# ---------------------------------------------------------------------------
etape "6/6 Lancement au démarrage"
if oui "Lancer LibrePods automatiquement quand tu ouvres ta session ?"; then
    mkdir -p "$HOME/.config/autostart"
    cat > "$HOME/.config/autostart/librepods.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=LibrePods
Exec=$BIN/librepods --start-minimized
X-GNOME-Autostart-enabled=true
EOF
    vert "OK"
fi

# ---------------------------------------------------------------------------
echo
vert "✅ Installation terminée."
if [ -d "$DONNEES/iphone_ecouteurs" ] && ls "$DONNEES/iphone_ecouteurs"/*.png >/dev/null 2>&1; then
    echo "L'animation est déjà en place. Lance LibrePods : $BIN/librepods"
else
    echo
    jaune "Dernière étape : l'animation (elle vient de TON iPhone)."
    echo "  1. Sur l'iPhone, lance un enregistrement d'écran."
    echo "  2. Ouvre le boîtier de tes AirPods près de l'iPhone : la pop-up apparaît."
    echo "  3. Laisse-la tourner au moins 15 secondes, puis arrête l'enregistrement."
    echo "  4. Mets la vidéo sur ton PC (câble, AirDrop, mail…) et lance :"
    echo "       python3 $DEPOT/outils/extraire_animation.py ta_video.mp4"
fi
