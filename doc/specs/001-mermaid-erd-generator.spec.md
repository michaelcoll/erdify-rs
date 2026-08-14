# Spec : Générateur de diagramme ER Mermaid depuis PostgreSQL

## Contexte

Le projet `erdify-rs` est un CLI Rust vide (stub) dont l'objectif est de se connecter à une base PostgreSQL et de générer un diagramme Entity-Relationship au format Mermaid (`erDiagram`). Le projet est déjà scaffoldé avec les dépendances principales (`clap`, `tokio-postgres`, `tokio`, `thiserror`) mais aucune logique métier n'est implémentée.

## Objectif

L'utilisateur passe une URL de connexion PostgreSQL au CLI, qui se connecte à la BDD, extrait le schéma (tables, colonnes, contraintes, relations), et restitue un bloc Markdown Mermaid `erDiagram` dans la sortie standard (ou un fichier).

## Solution

### Connexion

- Un flag `--url` (ou `-u`) accepte une URL PostgreSQL complète (ex: `postgresql://user:pass@host:5432/dbname`).
- Si `--url` n'est pas passé, le CLI lit la variable d'environnement `DATABASE_URL` définie dans `mise.toml`.
- Si ni l'un ni l'autre n'est fourni, affichage d'un message d'erreur clair indiquant comment fournir la connexion.
- Réutiliser `tokio-postgres` (déjà dans `Cargo.toml`) pour la connexion async.

### Filtrage des données

- **`--schema`** : filtre les tables par schéma PostgreSQL. Accepte une chaîne de schémas séparés par des virgules (ex: `--schema public,extended`). Par défaut, récupérer les tables de tous les user schemas (exclure `information_schema`, `pg_catalog`).
- **`--table`** : filtre les tables par nom. Accepte une chaîne de tables séparées par des virgules (ex: `--table users,orders`). Si présent, `--schema` est optionnel (recherche dans tous les schemas).
- **`--ignore-tables`** : exclut des tables par nom. Accepte une chaîne de tables séparées par des virgules (ex: `--ignore-tables logs,audit_trail`). S'applique après la sélection par `--schema` : les tables nommées sont retirées du résultat quel que soit leur schéma.
- `--table` et `--ignore-tables` sont mutuellement exclusifs (clap `conflicts_with`) : une liste blanche et une liste noire combinées n'ont pas de sens, l'utilisateur choisit l'une ou l'autre.
- `--schema` et `--ignore-tables` peuvent être combinés : `--schema public --ignore-tables logs` génère le diagramme de toutes les tables du schéma `public` sauf `logs`.
- `--schema` et `--table` peuvent être combinés : `--schema public --table users` sélectionne uniquement la table `users` du schéma `public`.

### Modes de sortie

Le niveau de détail du diagramme est contrôlé par l'absence de flag ou par des flags explicites :

- **Mode défaut (aucun flag)** : tables + colonnes avec types + clefs primaires (PK) + clefs étrangères (FK identifiées, mais relations non dessinées).
- **`--minimal`** : tables + colonnes avec types uniquement. Pas de PK ni FK.
- **`--full`** : tout le contenu du mode défaut + propriétés de colonnes (NOT NULL) + relations Mermaid avec cardinalités (`||--|{`, `|--||`, etc.) + contraintes (UNIQUE, CHECK) + indexes.

### Récupération du schéma

Interroger les vues système PostgreSQL (`information_schema.tables`, `information_schema.columns`, `pg_constraint`, `pg_class`, `pg_index`) pour extraire :

- Noms de tables et de colonnes
- Types de données
- Clefs primaires (`contype = 'p'`)
- Clefs étrangères (`contype = 'f'`)
- Contraintes UNIQUE (`contype = 'u'`) et CHECK (`contype = 'c'`)
- Index (via `pg_index` joint à `pg_class`)

### Sortie

- Par défaut, le résultat est écrit sur **stdout** sous forme de bloc Markdown :
  ```markdown
  # <Titre du diagramme>

  erDiagram
  ...
  ```
- **`--output <fichier>`** : option pour rediriger la sortie vers un fichier (ex: `erdify.md`). Crée le fichier s'il n'existe pas, l'écrase sinon.
- **`--title <texte>`** : titre optionnel du diagramme. Par défaut, utiliser le nom de la base de données extrait de l'URL de connexion (ex: `erdify-rs — public schema`).

### Structure CLI (clap derive)

Le CLI utilise `clap` avec derive pour les sous-commands ou arguments plats :

```
CLI tool to generate Mermaid ER diagrams from PostgreSQL databases

Usage: erdify [OPTIONS]

Options:
  -u, --url <URL>                    URL de connexion PostgreSQL (ex: postgresql://user:pass@host:5432/dbname)
      --schema <SCHEMA>              Schemas à inclure, séparés par des virgules (ex: public,extended)
      --table <TABLE>                Tables à inclure, séparées par des virgules (ex: users,orders)
      --ignore-table <IGNORE_TABLE>  Tables à exclure, séparées par des virgules (ex: logs,audit_trail)
      --minimal                      Mode minimal : colonnes uniquement, sans métadonnées PK/FK
      --full                         Mode complet : colonnes + PK/FK/NOT NULL + relations + contraintes + indexes
  -o, --output <OUTPUT>              Fichier de sortie (par défaut : stdout)
      --title <TITLE>                Titre du diagramme (par défaut : extrait du nom de la BDD)
  -h, --help                         Print help
  -V, --version                      Print version
```

- `--minimal` et `--full` sont mutuellement exclusifs (clap `conflicts_with`). Aucun des deux = mode défaut.
- `--table` et `--ignore-tables` sont mutuellement exclusifs (clap `conflicts_with`).

### Contraintes d'implémentation

- Utiliser `clap` avec le crate `derive` (déjà dans `Cargo.toml`).
- Utiliser `tokio-postgres` pour la connexion et les requêtes async.
- Utiliser `thiserror` pour les types d'erreur.
- Async complet avec `tokio` (`full` features).
- La connexion doit avoir un timeout (ex: 10s) pour ne pas bloquer indéfiniment.
- Gérer les cas où PostgreSQL retourne des types de données custom/unknown — les afficher tels quels ou avec un fallback `"unknown"`.

## Cas d'erreurs

- **URL manquante** : si ni `--url` ni `DATABASE_URL` ne sont fournis, exit avec code 1 et message d'erreur explicite.
- **Connexion échouée** : timeout, auth refusée, host inaccessible — message d'erreur descriptif avec le code de sortie 1.
- **Schema inexistant** : si `--schema` référence un schéma qui n'existe pas dans la BDD, afficher un avertissement et continuer avec les schemas trouvés (ou exit avec code 1 selon la stricte souhaitée).
- **Tables inexistantes** : si `--table` référence des tables qui n'existent pas dans les schemas spécifiés, afficher un avertissement avec les tables non trouvées.
- **Table à ignorer inexistante** : si `--ignore-tables` référence une table absente des schemas spécifiés, ne rien signaler (exclusion sans effet, pas une erreur).
- **`--table` et `--ignore-tables` combinés** : exit avec code 1 via l'erreur clap `conflicts_with`.
- **Toutes les tables ignorées** : si `--ignore-tables` exclut toutes les tables trouvées, se comporter comme le cas « BDD vide » (message clair, pas de crash).
- **BDD vide** : si aucune table n'est trouvée, afficher un message clair indiquant qu'aucune table n'a été détectée dans les schemas spécifiés.
- **Permissions insuffisantes** : si l'utilisateur n'a pas les droits de lecture sur certaines tables/schemas, afficher un avertissement avec les noms concernés.
- **Type de colonne inconnu** : afficher le type tel que retourné par PostgreSQL, avec fallback `"unknown"` si le type n'est pas reconnu.

## Critères d'acceptance

- [ ] L'outil s'exécute avec `--help` et affiche la liste complète des options (url, schema, table, ignore-tables, minimal, full, output, title).
- [ ] L'outil refuse de démarrer si ni `--url` ni `DATABASE_URL` ne sont fournis, avec un message d'erreur clair et un exit code 1.
- [ ] L'outil se connecte à une base PostgreSQL valide et genere un diagramme ER correct sur stdout.
- [ ] Le flag `--url postgresql://user:pass@host:5432/mydb` fonctionne et se connecte à la BDD spécifiée.
- [ ] La variable `DATABASE_URL` est utilisée comme fallback quand `--url` n'est pas passé.
- [ ] Le flag `--schema public` filtre le diagramme au seul schéma `public`.
- [ ] Le flag `--schema public,extended` inclut les tables des deux schemas.
- [ ] Le flag `--table users` filtre le diagramme à la seule table `users` (tous schemas confondus).
- [ ] Combiner `--schema public --table users` filtre à la table `users` dans le schéma `public`.
- [ ] Le flag `--ignore-tables logs` exclut la table `logs` du diagramme, quel que soit son schéma.
- [ ] Le flag `--ignore-tables logs,audit_trail` exclut plusieurs tables.
- [ ] Combiner `--schema public --ignore-tables logs` génère le diagramme des tables du schéma `public` sauf `logs`.
- [ ] Passer `--table` et `--ignore-tables` simultanément retourne une erreur clap (`conflicts_with`), exit code différent de 0.
- [ ] Si `--ignore-tables` référence une table qui n'existe dans aucun schema, aucun avertissement n'est affiché (pas d'effet, pas d'erreur).
- [ ] Si `--ignore-tables` exclut toutes les tables trouvées, le message « aucune table détectée » s'affiche (pas de crash).
- [ ] En mode défaut (aucun flag de mode), le diagramme contient les tables, colonnes avec types, et les PK/FK identifiés mais **sans** les relations de cardinalités Mermaid.
- [ ] Le flag `--minimal` génère un diagramme contenant uniquement les tables et colonnes avec types (pas de PK/FK).
- [ ] Le flag `--full` génère un diagramme contenant tables, colonnes, PK, FK, propriétés NOT NULL, relations Mermaid avec cardinalités, contraintes (UNIQUE, CHECK) et indexes.
- [ ] `--minimal` et `--full` sont mutuellement exclusifs (passer les deux retourne une erreur clap).
- [ ] Le flag `--output resultat.md` écrit le diagramme dans `resultat.md` au lieu de stdout.
- [ ] Le flag `--title "Mon Schéma"` définit le titre du diagramme.
- [ ] Sans `--title`, le titre est dérivé du nom de la base de données extrait de l'URL (ex: "erdify-rs — public schema").
- [ ] Le bloc Mermaid commence par `erDiagram` et respecte la syntaxe Mermaid valide (testable en le collant dans un render Mermaid).
- [ ] Les clefs primaires sont marquées avec `PK` dans la syntaxe Mermaid.
- [ ] Les clefs étrangères sont marquées avec `FK` dans la syntaxe Mermaid.
- [ ] En mode `--full`, les relations Mermaid utilisent les cardinalités correctes (ex: `||--|{ "Patients" }` pour 1:N).
- [ ] En mode `--full`, les indexes sont affichés (nom + colonnes cibles).
- [ ] Si `--schema` référence un schéma inexistant, un avertissement est affiché (pas de crash).
- [ ] Si `--table` référence des tables inexistantes, un avertissement liste les tables non trouvées.
- [ ] Si aucune table n'est trouvée, un message clair indique qu'aucune table n'a été détectée.
- [ ] L'outil timeout après 10 secondes si la connexion bloque, avec un message d'erreur clair.
- [ ] L'outil gère les types de colonnes personnalisés/unknown sans crash (fallback `"unknown"`).
