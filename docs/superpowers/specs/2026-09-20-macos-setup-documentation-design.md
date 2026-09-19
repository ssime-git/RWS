# Design — documentation de démarrage macOS

## Objectif

Permettre à un développeur sur un nouveau Mac de préparer et vérifier RWS sans
connaître la pile macFUSE/SSHFS à l'avance, soit en suivant les commandes lui-même,
soit en confiant les étapes répétitives à un agent générique. La documentation ne
doit pas promettre qu'un montage est validé avant un vrai cycle monter, modifier,
vérifier à distance et démonter.

## Architecture documentaire

Le README devient le point d'entrée concis. Il présente les prérequis, deux
parcours (agent générique et installation manuelle), les garanties de sécurité,
et un lien vers un seul guide de démarrage.

Un nouveau guide `docs/setup-macos.md` fournit une checklist séquentielle :

1. prérequis de développement, SSH et espace de test distant ;
2. installation séparée de macFUSE et SSHFS, puis construction du SSHFS
   expérimental requis par la route FSKit actuelle ;
3. activation FSKit et réglages Finder ;
4. enregistrement d'un espace de travail, montage et validation réelle ;
5. démontage, nettoyage et conservation privée des diagnostics.

Les documents existants gardent leur rôle de référence : `prototype.md` pour
l'utilisation de la CLI, `sshfs-fskit.md` pour le build compatible FSKit,
`finder-macos.md` pour Finder, `fskit-debugging.md` pour le diagnostic, et
`validation.md` pour l'état de preuve. Ils renverront vers le guide de démarrage
lorsqu'un lecteur cherche une procédure d'installation complète.

## Parcours avec agent

Le parcours est compatible avec tout agent capable de lire le dépôt et
d'exécuter des commandes locales. Il lui demande de suivre le guide, de conserver
la configuration et les preuves privées dans `.rws-local/`, et de rapporter les
étapes non vérifiées.

L'agent ne doit jamais récupérer ou modifier des identifiants SSH, contourner la
vérification de clés hôte, installer silencieusement des logiciels système, ni
changer les autorisations macOS sans accord explicite. L'utilisateur effectue les
étapes qui demandent mot de passe, acceptation de licence, confiance SSH ou choix
de réglage système.

## Autorisations et sécurité

La documentation distingue :

- les permissions normalement requises : autoriser les installateurs macOS et
  activer les extensions FSKit macFUSE dans Réglages Système ;
- les préférences Finder facultatives pour afficher/ajouter le volume à la barre
  latérale ;
- Full Disk Access, qui n'est pas une étape normale : il peut être temporairement
  nécessaire uniquement pour diagnostiquer/réparer un refus de macOS lors de
  l'activation FSKit, avec accord explicite, sauvegarde, puis retrait vérifié ;
- les actions explicitement hors procédure : réduction de la sécurité de démarrage,
  activation du backend noyau hérité, `chmod`/`chown` pour contourner la vie privée,
  ou exécution aveugle d'un script communautaire.

## Validation et erreurs

Le guide explicite les contrôles concrets : outils disponibles, SSH et SFTP,
exécutable SSHFS fonctionnel, module FSKit activé, contenu monté, création et
modification locale, lecture distante indépendante, puis démontage. Les erreurs
renvoient au document de diagnostic adapté sans proposer de réparation destructive.

## Hors périmètre

Cette mise à jour ne modifie ni l'application macOS en cours de développement, ni
le comportement de la CLI, ni les scripts d'installation. Elle ne revendique pas
la validation des sauvegardes d'éditeur complexes, de la récupération réseau, ou
de la persistance automatique de la barre latérale Finder.
