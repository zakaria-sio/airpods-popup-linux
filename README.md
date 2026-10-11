# AirPods Popup pour Linux (LibrePods)

Ouvre le boîtier de tes AirPods à côté de ton PC Linux : **une pop-up façon iPhone apparaît**, avec les écouteurs et le boîtier qui tournent, et la batterie de chacun.

> 🇬🇧 English summary at the bottom.

Basé sur [LibrePods](https://github.com/kavishdevar/librepods), qui fait le gros du travail (connexion, batterie, modes d'écoute). Ce dépôt ajoute la pop-up par-dessus.

---

## Ce que ça fait

- **Couvercle ouvert → la pop-up s'ouvre.** Couvercle fermé → elle se ferme.
- **Les 2 écouteurs dans les oreilles → elle se ferme toute seule.**
- **Clic sur l'icône LibrePods** (dans la barre en haut) → les écouteurs, la batterie et un bouton **Réglages du son** :
  Réduction du bruit · Adaptatif · Transparence · Désactivé.
- **Bouton Connecter** : le PC se connecte aux AirPods **seulement si tu le demandes**. Il ne les « vole » pas à ton iPhone.

Testé sur **Ubuntu 26.04** (GNOME) avec des **AirPods Pro 3**.
Les autres modèles (AirPods Pro 2, AirPods 4…) devraient marcher, mais n'ont pas été testés : dis-le dans une *issue* si tu essaies.

---

## ⚠️ Pourquoi il n'y a aucune image dans ce dépôt

Les images des AirPods appartiennent à Apple. On n'a pas le droit de les publier.

À la place, ce dépôt contient un **script** : tu filmes l'écran de **ton** iPhone quand la pop-up s'affiche, et le script découpe tout seul les images. Les images restent sur ton PC.

---

## Installation

Il faut : **Ubuntu ou Debian**, un adaptateur Bluetooth, et **Rust**.
Si tu n'as pas Rust :
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```
(puis ferme et rouvre le terminal)

Ensuite :
```bash
git clone https://github.com/zakaria-sio/airpods-popup-linux.git
cd airpods-popup-linux
bash install.sh
```

Le script :
1. vérifie les outils et propose de les installer ;
2. télécharge LibrePods (version testée) ;
3. applique les patchs (dossier `patches/`) ;
4. compile LibrePods et la pop-up (10 à 20 min la 1re fois) ;
5. active le mode « expérimental » du Bluetooth (il te demande avant) ;
6. propose de lancer LibrePods au démarrage.

Rien n'est installé en dehors de ton dossier perso, sauf les paquets `apt` et le réglage Bluetooth, et **il te demande toujours avant**.

---

## Fabriquer l'animation (avec ton iPhone)

**1. Filme l'écran de l'iPhone**
- Centre de contrôle → bouton **Enregistrement de l'écran** ⏺.
- Ouvre le boîtier de tes AirPods près de l'iPhone : la pop-up apparaît.
- **Ne touche à rien pendant au moins 15 secondes** (les objets doivent faire plusieurs tours).
- Arrête l'enregistrement.

**2. Mets la vidéo sur ton PC**
Câble, AirDrop, Google Drive, mail… Évite **WhatsApp** : il compresse la vidéo, et les images seront floues.

**3. Lance le script**
```bash
python3 outils/extraire_animation.py ~/Téléchargements/ta_video.MP4
```

Il trouve tout seul :
- la carte de la pop-up ;
- les écouteurs (à gauche) et le boîtier (à droite) ;
- la boucle de rotation, pour que l'animation tourne sans saut.

Il range les images dans `~/.local/share/airpods_popup/`. C'est là que la pop-up les cherche.

**4. Vérifie**
Ouvre `~/.local/share/airpods_popup/apercu_decoupage.png` :
- à gauche, les cadres de couleur doivent entourer la carte, les écouteurs et le boîtier ;
- à droite, tu dois voir quelques images de l'animation.

---

## Si ça ne marche pas

| Problème | Solution |
|---|---|
| Le script ne trouve pas la carte | Coupe le début ou la fin de la vidéo : `--debut 2 --fin 20` (en secondes) |
| « raccord visible » | Ta vidéo est trop courte : refais-la avec 20-30 s de pop-up sans toucher |
| La pop-up ne s'ouvre pas au boîtier | Vérifie `Experimental = true` dans `/etc/bluetooth/main.conf`, puis `sudo systemctl restart bluetooth` |
| Le PC prend les AirPods à l'iPhone | Dans `~/.config/librepods/preferences.json`, mets `{"AA:BB:CC:DD:EE:FF": {"autoConnect": false}}` (avec l'adresse de tes AirPods, visible avec `bluetoothctl devices`), puis utilise le bouton **Connecter** de la pop-up |
| Voir ce qui se passe | Lance LibrePods dans un terminal : `RUST_LOG=info ~/.local/bin/librepods` |

---

## Comment ça marche

```
 AirPods ──Bluetooth──▶ LibrePods (patché)
                          │  écrit   ~/.local/share/airpods_popup/etat.txt
                          │          (nom, batteries, couvercle, oreilles, mode d'écoute)
                          │  lance / ferme
                          ▼
                       airpods-popup (fenêtre iced)
                          │  écrit   commande.txt  (ex : ecoute=2)
                          └────────▶ LibrePods l'envoie aux AirPods
```

- **`patches/`** : les modifications de LibrePods (une par fichier, chacune expliquée en haut du fichier).
- **`popup/`** : la fenêtre de la pop-up (Rust + [iced](https://iced.rs)).
- **`outils/extraire_animation.py`** : le découpage de la vidéo (Python + ffmpeg).

| Patch | Rôle |
|---|---|
| 01 | Enregistre les clés Bluetooth des AirPods (nécessaire pour lire leurs signaux) |
| 02 | Batterie du boîtier fiable |
| 03 | Détecte couvercle ouvert / fermé |
| 04 | Ouvre / ferme la pop-up |
| 05-06 | Infos pour la pop-up (batteries, écouteurs dans le boîtier) |
| 07 | Une seule pop-up à la fois, fermeture quand les écouteurs sont dans les oreilles, clic sur l'icône |
| 08 | Menu « Afficher la pop-up » + bouton Connecter |
| 09 | Changer le mode d'écoute depuis la pop-up |
| 10 | Chemin de la pop-up installée |

---

## Désinstaller

⚠️ La 2ᵉ ligne supprime aussi tes images découpées (il faudra relancer le script si tu réinstalles).

```bash
rm ~/.local/bin/librepods ~/.local/bin/airpods-popup ~/.config/autostart/librepods.desktop
rm -r ~/.local/share/airpods_popup ~/.local/share/librepods-popup
```

---

## Licence et crédits

- [LibrePods](https://github.com/kavishdevar/librepods) par kavishdevar et ses contributeurs — **AGPL-3.0**.
- Ce dépôt modifie LibrePods, donc il est aussi sous **AGPL-3.0** (voir `LICENSE`).
- AirPods, iPhone et Apple sont des marques d'Apple Inc. Ce projet n'est pas lié à Apple. Aucune image Apple n'est distribuée ici.

---

## 🇬🇧 English summary

An iPhone-style AirPods popup for Linux, built on top of [LibrePods](https://github.com/kavishdevar/librepods).
Open the case → the popup shows the rotating earbuds and case with battery rings. It closes when you close the lid or put both earbuds in. Clicking the tray icon shows the battery and lets you switch listening modes (ANC / Adaptive / Transparency / Off).

**No Apple images are included.** Record your own iPhone screen while the AirPods popup is showing (15 s or more, don't touch anything). Then run `python3 outils/extraire_animation.py your_video.mp4`. It finds the card, the earbuds and the case by itself, removes the background, and detects the rotation loop.

Install on Ubuntu/Debian: `bash install.sh` (needs Rust). Tested on Ubuntu 26.04 with AirPods Pro 3. License: AGPL-3.0.
