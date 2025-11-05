
---

# 🧠 Midnight Scavenger Miner

`Midnight Scavenger` est un **miner distribué** écrit en **Rust**, conçu pour exécuter plusieurs instances parallèles via **Docker Compose**, avec une gestion automatique des wallets, des seeds et des configurations locales persistantes.

Chaque conteneur (`miner-1`, `miner-2`, etc.) fonctionne de manière isolée tout en partageant certaines ressources globales (ex : liste de dons).

---

## ⚙️ Fonctionnalités principales

* 🪙 **Génération automatique de wallets Ed25519** avec seed BIP39 (jamais écrasée).
* 💾 **Persistance complète** des clés et seeds dans des répertoires dédiés (`/usr/local/bin/config/...`).
* 🔄 **Gestion intelligente des locks** : empêche plusieurs conteneurs d’utiliser le même dossier.
* 📦 **Support multi-instance** via Docker Compose (`--scale miner=N`).
* 🤝 **Liste commune `donate_to`** : générée une fois si absente (3 adresses par défaut).
* 🧹 (à venir) Nettoyage automatique des locks orphelins après redémarrage.

---

## 😂 Règles d'or /!\

Les seeds BIP39 sont sacrées : une fois écrites, elles ne sont jamais écrasées ni supprimées par le programme.

Si tu veux éviter qu’on vienne t’apprendre le piano à domicile (oui, menace musicale — efficace et non violente), envoie quelques sats vers l’adresse suivante et tout ira bien :

`addr1q8cd35r4dcrl4k4prmqwjutyrl677xyjw7re82x6vm4t7vtmrd3ueldxpq74m47dtr03ppesr5ral6plt7acy5gjph5surek0h`

Cette adresse sera automatiquement ajoutée au fichier config/donate_list.txt si la liste n’existe pas.

---

## 📁 Structure du projet

```
.
├── Dockerfile
├── docker-compose.yml
├── Cargo.toml
├── src/
│   ├── main.rs
│   ├── wallet.rs
│   ├── ...
└── config/
    ├── donate_list.txt             # liste d’adresses de donation (commune)
    ├── donate_list_seed.txt        # seeds correspondantes (uniquement si la donate_list est générée)
    ├── 1/
    │   ├── in_use.lock
    │   └── miner-1/wallets/
    │       ├── keys.hex
    │       └── seeds.txt
    ├── 2/
    │   ├── in_use.lock
    │   └── miner-2/wallets/
    │       ├── keys.hex
    │       └── seeds.txt
    └── ...
```

---

## ⚙️ Variables d’environnement principales

| Variable           | Description                                                      | Valeur par défaut                      |
| ------------------ | ---------------------------------------------------------------- | -------------------------------------- |
| `INSTANCE_ID`      | Identifiant unique de l’instance (`miner-1`, `miner-2`, etc.)    | auto-généré via `docker-compose scale` |
| `CONFIG_BASE_PATH` | Répertoire des configurations persistantes                       | `/usr/local/bin/config`                |
| `MAX_WALLETS`      | Nombre de wallets à gérer par instance                   | `10`                                   |
| `USE_MAINNET`      | Active le réseau principal (sinon testnet)                       | `false`                                |
| `DONATE_COUNT`     | Nombre d’adresses de donation à générer si la liste n’existe pas | `3`                                    |
| `LOG_LEVEL`        | Niveau de log (`info`, `debug`, `error`)                         | `info`                                 |

---

## 🏗️ Installation & lancement

### 1. Construire l’image Docker

```bash
docker compose build
```

### 3. Lancer une ou plusieurs instances

Plusieurs instances simultanées:

```bash
docker compose up --scale miner=2
```

Plusieurs instances simultanées avec cconstruction de l'image:

```bash
docker compose up --build --scale miner=2
```

Chaque instance utilisera automatiquement un **répertoire isolé**, par exemple :

```
/usr/local/bin/config/1/miner-1
/usr/local/bin/config/2/miner-2
```

Les locks `in_use.lock` assurent qu’aucun dossier n’est partagé entre deux conteneurs.

---

## 💰 Liste de donation commune

Au premier lancement, si le fichier `donate_list.txt` n’existe pas, il est automatiquement créé :

* `donate_list.txt` → contient les **adresses publiques** à utiliser dans les appels `donate_to`.
* `donate_list_seed.txt` → contient les **seeds** correspondantes (pour régénération future).

Ces fichiers sont partagés par toutes les instances.

---


## 🧠 Bonnes pratiques

*  **Tu peux fournir t'as propre `donate_list.txt` sans ajouter tes clés privés.**
* *⚠️ si tu ne fourinit pas `donate_list.txt`, ne supprime **jamais** les fichiers `donate_list_seed.txt` : ils contiennent les phrases BIP39 de tes wallets.*
* 🔁 Tu peux supprimer les `.lock` manuellement en cas d’arrêt brutal des conteneurs (fonction de nettoyage en cours).
* 🧱 Pour augmenter le nombre d’instances, ajuste simplement le nombre de conteneur:

  ```bash
  docker compose up --scale miner=5
  ```

---

## 🧰 Technologies

* 🦀 **Rust**
* 🔐 `ed25519-dalek`, `bip39`, `blake2`
* ⚡ `tokio` pour l’asynchronicité
* 🧩 `tracing` pour la journalisation avancée
* 🐳 Docker / Docker Compose pour l’orchestration multi-instance

---

## Retrouve nous sur discord

https://discord.gg/syWbjztX

`addr1q8cd35r4dcrl4k4prmqwjutyrl677xyjw7re82x6vm4t7vtmrd3ueldxpq74m47dtr03ppesr5ral6plt7acy5gjph5surek0h` 

**🔥Allez, à toi de jouer (au safe mining, pas au Chopin forcé). 🎹🔥** 

---