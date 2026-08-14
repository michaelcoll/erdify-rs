# Spec : Support des vues et des valeurs par défaut de colonnes

## Contexte

L'outil `erdify-rs` interroge le catalogue système PostgreSQL (`fetch_table_list` dans
`src/db.rs`) en filtrant sur `c.relkind IN ('r', 'p')` : seules les tables ordinaires et
partitionnées sont extraites. Les vues (`relkind = 'v'`) et vues matérialisées
(`relkind = 'm'`) sont totalement absentes du diagramme généré, même si elles font partie
du schéma applicatif de l'utilisateur.

Par ailleurs, la structure `Column` (`src/schema.rs`) ne porte que `name` et `data_type` :
la valeur par défaut d'une colonne (`DEFAULT ...`), pourtant disponible dans le catalogue
(`pg_attrdef`), n'est jamais récupérée ni affichée, y compris en mode `--full` qui est
censé donner la vue la plus complète du schéma.

## Objectif

- Les vues et vues matérialisées doivent apparaître dans le diagramme au même titre que
  les tables, sans flag supplémentaire à passer, et être identifiables comme telles.
- Les valeurs par défaut des colonnes de table doivent être visibles en mode `--full`.

## Solution

### Vues et vues matérialisées comme entités

- Étendre la requête de `fetch_table_list` pour inclure `relkind IN ('r', 'p', 'v', 'm')`.
- Le type d'entité (table, vue, vue matérialisée) doit être conservé sur chaque `Table`
  (ou structure équivalente) issue du catalogue, pour permettre de les classer ensuite.
- Colonnes : la requête existante sur `pg_attribute` s'applique déjà aux vues et vues
  matérialisées sans modification.
- Contraintes PK/FK/UNIQUE/CHECK : PostgreSQL n'autorise pas ces contraintes sur les vues
  ni les vues matérialisées ; les requêtes `pg_constraint` existantes continuent de
  s'exécuter sans changement et ne remontent naturellement rien pour elles.
- Index : les vues matérialisées peuvent porter des index (y compris uniques) ; la requête
  `pg_index` existante doit continuer à s'appliquer à elles pour que le marqueur `UK`
  reste correct en mode `--full`. Les vues simples n'ont jamais d'index.
- Filtrage : les vues et vues matérialisées passent par le même pipeline que les tables
  (`--schema`, `--table`, `--ignore-tables`, exclusion des schémas système). Aucun
  nouveau flag n'est introduit pour les exclure globalement ; `--ignore-tables` reste le
  seul moyen de les exclure nommément.
- Relations Mermaid (mode `--full`) : aucune règle spécifique à ajouter — l'absence de FK
  en catalogue pour les vues/vues matérialisées fait qu'aucune relation n'est dessinée
  pour elles, comme conséquence naturelle du comportement existant.
- Une nouvelle section Markdown est ajoutée après le bloc Mermaid, sur le modèle de la
  section « Indexes and constraints » existante : elle liste les entités qui sont des
  vues et vues matérialisées, avec leur type. Cette section apparaît dans **tous les
  modes de sortie** (`--minimal`, défaut, `--full`) dès qu'au moins une vue ou vue
  matérialisée figure dans le résultat filtré ; elle est absente sinon.

### Valeurs par défaut des colonnes

- Étendre la récupération des colonnes pour inclure la valeur par défaut telle
  qu'exposée par le catalogue (`pg_attrdef`, jointure sur `attrelid`/`attnum`), au même
  endroit que la requête existante sur `pg_attribute` dans `fetch_columns`.
- Applicable uniquement aux colonnes de tables ordinaires/partitionnées : les vues et
  vues matérialisées n'ont pas de valeur par défaut de colonne au sens PostgreSQL (leurs
  colonnes sont dérivées de la requête sous-jacente) — aucune récupération à faire pour
  elles, la colonne reste simplement sans défaut.
- Affichage réservé au mode `--full`, sur le même principe que `NOT NULL` aujourd'hui :
  l'information apparaît en annotation de la colonne concernée, uniquement quand une
  valeur par défaut existe.
- La valeur est affichée telle que retournée par PostgreSQL (ex: `nextval('users_id_seq'::regclass)`,
  `now()`, `'draft'::character varying`), sans troncature, avec le même nettoyage que les
  autres textes libres du diagramme (guillemets et retours à la ligne neutralisés) pour
  rester une chaîne Mermaid valide.

## Cas d'erreurs

- **Vue ou vue matérialisée sans colonnes** : même traitement qu'une table sans colonnes
  aujourd'hui (bloc d'entité déclaré sans attributs, pas de crash).
- **Vue matérialisée non rafraîchie ou invalide** : n'est pas traitée différemment ;
  l'outil expose la structure telle qu'elle existe dans le catalogue, indépendamment de
  son état de population.
- **Colonne sans valeur par défaut** : rien n'est affiché pour cette colonne (comme
  aujourd'hui pour `NOT NULL` absent).
- **Valeur par défaut contenant des guillemets ou retours à la ligne** : neutralisée par
  le même nettoyage que les labels de relations, pour ne pas casser la syntaxe Mermaid.
- **Base sans aucune vue ni vue matérialisée** : la nouvelle section dédiée n'apparaît
  pas, comme la section « Indexes and constraints » qui ne s'affiche que si du contenu
  existe.
- **`--schema`/`--table` ne ciblant que des vues** : fonctionne comme pour des tables,
  sans traitement particulier.

## Critères d'acceptance

- [ ] Sans flag supplémentaire, une vue présente dans le schéma filtré apparaît comme
      entité dans le diagramme Mermaid.
- [ ] Sans flag supplémentaire, une vue matérialisée présente dans le schéma filtré
      apparaît comme entité dans le diagramme Mermaid.
- [ ] Les tables ordinaires et partitionnées continuent d'apparaître exactement comme
      avant (pas de régression).
- [ ] La section dédiée listant les vues/vues matérialisées est présente en mode
      `--minimal`, en mode par défaut et en mode `--full` dès qu'au moins une vue ou vue
      matérialisée figure dans le résultat.
- [ ] La section dédiée est absente quand aucune vue ni vue matérialisée n'est présente
      dans le résultat filtré.
- [ ] La section dédiée distingue les vues simples des vues matérialisées.
- [ ] `--schema public` filtre aussi les vues et vues matérialisées du schéma `public`.
- [ ] `--table ma_vue` sélectionne une vue nommée `ma_vue` au même titre qu'une table.
- [ ] `--ignore-tables ma_vue` exclut la vue `ma_vue` du diagramme et de la section
      dédiée.
- [ ] En mode `--full`, aucune relation Mermaid (`||--o{`, etc.) n'est dessinée en
      direction ou en provenance d'une vue ou vue matérialisée.
- [ ] En mode `--full`, une vue matérialisée dotée d'un index unique affiche le marqueur
      `UK` sur la colonne concernée, comme une table.
- [ ] En mode `--full`, une colonne de table avec une valeur par défaut affiche cette
      valeur en annotation (ex: `default: now()`).
- [ ] En mode `--full`, une colonne de table sans valeur par défaut n'affiche aucune
      annotation de défaut.
- [ ] En mode `--minimal` et en mode par défaut, aucune valeur par défaut n'est affichée,
      même si la colonne en a une.
- [ ] Une colonne de vue ou de vue matérialisée n'affiche jamais de valeur par défaut,
      même en mode `--full`.
- [ ] Une valeur par défaut contenant un guillemet double ou un retour à la ligne est
      affichée sans casser la syntaxe du bloc Mermaid (rendu Mermaid valide, testable en
      collant la sortie dans un moteur de rendu Mermaid).
- [ ] Une colonne à la fois `NOT NULL` et dotée d'une valeur par défaut affiche les deux
      informations en mode `--full`.
