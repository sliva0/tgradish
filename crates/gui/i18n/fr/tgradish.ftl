### tgradish's words in French. What each message means and where it is
### shown is said in the comments of the English file, en-GB/tgradish.ftl.

language-name = Français


## Shared by many places

close = Fermer
back-to-preset = Revenir à la valeur du préréglage
custom = Personnalisé
    .hint = Choisissez « { custom } » pour le définir
webm-sticker = Sticker WebM
webm-emoji = Émoji WebM
tgs-sticker = Sticker TGS
tgs-emoji = Émoji TGS
image-sticker = Sticker statique
image-emoji = Émoji statique
count-colours = { $count ->
    [one] { $count } couleur
    [many] { $count } couleurs
   *[other] { $count } couleurs
}
count-frames = { $count ->
    [one] { $count } image
    [many] { $count } images
   *[other] { $count } images
}
count-layers = { $count ->
    [one] { $count } calque
    [many] { $count } calques
   *[other] { $count } calques
}
count-rectangles = { $count ->
    [one] { $count } rectangle
    [many] { $count } rectangles
   *[other] { $count } rectangles
}
count-tracks = { $count ->
    [one] { $count } piste
    [many] { $count } pistes
   *[other] { $count } pistes
}
of-limit = { $used } sur { $limit }
telegram-accepts = Telegram devrait l'accepter


## Messages along the bottom

dismiss = Masquer
    .all = Tout masquer
said-times = ×{ $count }
details = Détails
settings-unreadable = Le fichier de paramètres est illisible : les paramètres par défaut sont utilisés
setting-unsaved = Votre modification n'a pas pu être enregistrée dans le fichier de paramètres
    .unreadable = Modification non enregistrée : le fichier de paramètres est illisible
settings-file-note = Il a peut-être été modifié à la main ou par une autre version de tgradish. Aucune modification ne peut être enregistrée tant qu'il n'est pas corrigé ou remplacé ; rien de son contenu n'est perdu.
fresh-settings = Repartir de zéro
    .hint = Écrire les paramètres actuels dans un nouveau fichier de paramètres ; l'ancien est conservé à côté
    .saved = Nouveaux paramètres enregistrés
    .kept = L'ancien fichier est conservé à côté, sous le nom { $name }
    .failed = Impossible d'écrire les nouveaux paramètres
read-again = Réessayer
    .hint = Le relire, une fois corrigé
    .failed = Toujours illisible
settings-read = Le fichier de paramètres est de nouveau lisible, et ses paramètres sont utilisés
copy-path = Copier le chemin
    .hint = Copier l'emplacement du fichier, pour l'ouvrir dans un éditeur
    .done = Copié
files-removed = { $count ->
    [one] { $count } fichier retiré
    [many] { $count } fichiers retirés
   *[other] { $count } fichiers retirés
}
undo-removing = Annuler
    .hint = Les remettre à leur place, avec leurs paramètres et résultats (Ctrl+Z)
settings-reset = Les paramètres sont revenus à leurs valeurs par défaut
undo-reset = Annuler
    .hint = Remettre les paramètres comme avant
folder-of = Afficher dans le dossier
    .hint = Ouvrir le dossier qui le contient
presets-unreadable = Impossible de lire vos préréglages : seuls les préréglages intégrés sont proposés
files-gone = { $count ->
    [one] { $count } fichier a disparu depuis la dernière fois
    [many] { $count } fichiers ont disparu depuis la dernière fois
   *[other] { $count } fichiers ont disparu depuis la dernière fois
}
session-unreadable = Impossible de lire les fichiers de la dernière fois : aucun n'est ouvert ; leur liste est mise de côté
files-unreadable = { $count ->
    [one] { $count } fichier de la dernière fois est illisible et a été écarté ; la liste d'origine est mise de côté
    [many] { $count } fichiers de la dernière fois sont illisibles et ont été écartés ; la liste d'origine est mise de côté
   *[other] { $count } fichiers de la dernière fois sont illisibles et ont été écartés ; la liste d'origine est mise de côté
}
session-unsaved = Impossible de conserver les fichiers ouverts pour la prochaine fois
nothing-to-paste = Rien à coller
    .why = Le presse-papiers ne contient ni fichier, ni chemin, ni image
paste-failed = Impossible de lire le presse-papiers
    .image = Impossible d'enregistrer l'image collée
pick-failed = La boîte de dialogue de fichiers a planté : rien n'a été choisi


## Dropping files

drop-to-add = Déposez pour ajouter des fichiers


## The system's file dialogs: their titles, which say what each is for,
## and the names of the kinds of files they show

dialog-add-files = Ajouter des fichiers à convertir
dialog-add-folder = Ajouter un dossier d'images
dialog-add-images = Ajouter des images en une seule animation
dialog-inspect = Choisir un sticker à inspecter
dialog-ffmpeg = Choisir ffmpeg
dialog-results-folder = Choisir où enregistrer les résultats
images-filter = Images
stickers-filter = Stickers


## The list of files, on the left

settings = Paramètres
light-theme = Passer au thème clair
dark-theme = Passer au thème sombre
main-menu = Paramètres et plus
menu-undo = Annuler
    .hint = La dernière modification des paramètres du fichier, ou les fichiers tout juste retirés
menu-redo = Rétablir
    .hint = La dernière modification annulée
menu-settings = Paramètres…
inspect = Inspecter un sticker…
    .hint = Vérifier un fichier au regard des limites de Telegram
feedback = Envoyer un avis
    .hint = Écrire à l'auteur sur Telegram : dans l'application si elle est installée, sinon dans le navigateur
about-and-licences = À propos et licences…
show-input-in-folder = Afficher dans le dossier
    .pasted = Une image collée n'a pas de fichier propre
shortcuts = Raccourcis clavier
add-files = Ajouter des fichiers…
add-more = Autres façons d'ajouter des fichiers
add-folder = Un dossier d'images…
    .hint = Par ordre de nom, en une seule animation
add-images = Des images en une animation…
    .hint = Les images choisies, par ordre de nom
add-pasted = Depuis le presse-papiers
    .hint = Fichiers copiés, chemins ou image
convert-all = { $count ->
    [0] Tout convertir
   *[other] Tout convertir ({ $count })
}
    .hint = Convertir chaque fichier sans résultat à jour
remove-all = Tout retirer
converting-batch = Conversion { $current } sur { $total }
stop-all = Tout arrêter
    .hint = Arrêter la conversion et retirer de la file les fichiers en attente
remove = Retirer
remove-file = Le retirer de la liste
convert = Convertir
    .hint = Créer son sticker ou son émoji
row-convert-again = Obsolète : reconvertir
row-next = suivant
row-waiting = en attente, { $ahead } devant
row-stale = { $size } · obsolète
row-failed = échec
row-exists = un fichier occupe la place
row-stopped = arrêté
drop-here = Déposez ici des vidéos, des GIF, des images ou des fichiers Aseprite
drop-here-note = Pour en faire un sticker ou un émoji vidéo, animé ou statique
paste = Coller
name-pasted = image collée
name-frames = { $name } (images)
name-and-more = { $name } et { $more } de plus
no-font = { $count ->
    [1] Aucune police, ni celles de tgradish ni celles du système, ne contient { $codes } : ce caractère est donc affiché par son code. Le fichier lui-même est intact
   *[other] Aucune police, ni celles de tgradish ni celles du système, ne contient { $codes } : ces caractères sont donc affichés par leur code. Le fichier lui-même est intact
}


## The bar under the settings of the file picked

stop = Arrêter
stopping = Arrêt…
keep-best = Garder le meilleur
    .hint = Cesser d'essayer d'autres encodages et enregistrer comme résultat le meilleur qui tient jusqu'ici
    .nothing-fits = Rien ne tient encore : aucun encodage à garder
keeping-best = Conservation du meilleur…
cancel-waiting = Annuler
next-to-convert = Prochain à convertir
waiting-for = En attente, { $ahead } devant
convert-again = Reconvertir
    .changed = Obsolète : le refaire avec les paramètres actuels
    .hint = Le refaire avec les mêmes paramètres
-save-settings = Enregistrer les paramètres comme préréglage
save-preset = { -save-settings }…
    .hint = Les garder comme préréglage personnel, point de départ pour n'importe quel fichier
failed = Échec : { $error }
    .short = Échec — voir les détails
in-the-way = Un fichier qui ne vient pas de tgradish occupe la place :
replace-it = Le remplacer
stopped = Arrêté
show-in-folder = Afficher dans le dossier
saves-as = Sera enregistré sous
    .hint = L'emplacement des résultats se règle dans « { settings } »
result-saved-as = Enregistré sous
copy-result = Copier
    .hint = Copier le sticker en tant que fichier, pour le coller dans une discussion Telegram
    .done = Copié
    .failed = Non copié
replaces-earlier = Remplace un résultat précédent
    .any = Remplace le fichier existant


## Reading a file to show it

reading-crashed = tgradish a planté en lisant ce fichier
reading-stopped = La lecture s'est arrêtée
no-frames = ffmpeg n'y a trouvé aucune image
no-frames-there = ffmpeg n'a trouvé aucune image à cet endroit
no-frame-there = ffmpeg n'a pas trouvé d'image à cet endroit
art-no-frames = Il ne contient aucune image
result-no-frames = Le résultat ne contient aucune image


## The result's small picture over the preview

inset-label = Résultat à { $scale }
    .plain = Résultat
    .drag = Faites-le glisser vers un autre coin
inset-scale = { $percent } %
    .times = ×{ $times }
inset-smaller = Plus petit
    .same = Il ne s'afficherait pas plus petit
inset-larger = Plus grand
    .same = Il ne s'afficherait pas plus grand


## Over the preview

show-input = Source
show-result = Résultat
    .outdated = Résultat ⚠
    .hint = Obsolète : le résultat a été créé avec des paramètres antérieurs
convert-first = Convertissez-le d'abord
tgs-unseen = Ce sticker TGS contient des formes que tgradish ne sait pas dessiner
result-preview = Aperçu du résultat
    .hint = Afficher / masquer le petit aperçu à côté de la source
    .input-only = Affiché à côté de la source, pas du résultat
zoom-out = Dézoomer
    .hint = Tout afficher
    .all = Déjà entièrement dézoomé
input-hint = Glisser pour recadrer · molette pour zoomer · clic droit pour se déplacer
crop-hint = Glisser le cadre ou ses bords · double-clic pour tout prendre
result-hint = Molette pour zoomer · clic droit pour se déplacer
unreadable = Lecture impossible : { $error }
reading = Lecture…
preview-divider = Séparateur sous l'aperçu


## The timeline under the preview

play = Lecture
    .pause = Pause
play-keys =
    Espace — lecture ou pause
    ← / → — une image en arrière ou en avant
    Maj+← / → — dix images en arrière ou en avant
    Début / Fin — première ou dernière image utilisée
    {"["} / ] — commencer ou finir la partie utilisée à l'image affichée
    Ctrl+← / → — déplacer le début de la partie utilisée
    Alt+← / → — déplacer la fin de la partie utilisée
nothing-to-play = Une seule image est utilisée : rien à lire
in-seconds = Secondes
    .hint = Temps en secondes
in-frames = Images
    .hint = Temps en numéros d'image, à partir de 1
    .unknown = La position des images de la source est inconnue
part-start = Début de la partie utilisée
    .frame = Début de la partie utilisée : sa première image
part-end = Fin de la partie utilisée
    .frame = Fin de la partie utilisée : sa dernière image
frame-number = image { $number }
limit-mark = Limite de 3 secondes de Telegram : la partie utilisée au-delà est coupée
    .spoofed = Limite de 3 secondes de Telegram : la partie utilisée au-delà est lue aussi, car Telegram croit le sticker plus court
    .sped-up = Limite de 3 secondes de Telegram : une partie utilisée plus longue est accélérée pour y tenir
frame-before = Image précédente (←)
    .first = C'est la première image
frame-after = Image suivante (→)
    .last = C'est la dernière image
frame-picked = L'image


## What to make, beside the preview

make = Créer
format-video = Vidéo
    .hint = Un sticker ou un émoji vidéo : du VP9 en WebM, à partir de n'importe quelle vidéo ou image
format-animation = Animé
    .hint = Un sticker ou un émoji animé : un fichier TGS. Cette option est conçue pour le pixel art : TGS est un format vectoriel, le résultat reste donc net à toute taille. Idéal pour les émojis personnalisés, qui ne font que 100 × 100 px. Pas pour les vidéos ou les photos ordinaires : utilisez plutôt « { format-video } »
format-image = Statique
    .hint = Un sticker ou un émoji statique : du WebP, à partir de n'importe quelle image ou d'une image d'une vidéo
target-sticker = Sticker
target-emoji = Émoji personnalisé
needs-webm-sticker = WebM, 512 px sur le côté le plus long, jusqu'à 256 KiB
needs-webm-emoji = WebM, 100 × 100 px, jusqu'à 64 KiB
needs-tgs = TGS, carré, jusqu'à 64 KiB
needs-image-sticker = WebP, 512 px sur le côté le plus long, jusqu'à 512 KiB
needs-image-emoji = WebP, 100 × 100 px, jusqu'à 512 KiB
not-pixel-art = « { format-animation } » est conçu pour le pixel art, converti en vectoriel pour rester net. Ceci ne ressemble pas à du pixel art : « { format-video } » lui convient mieux
preset = Préréglage
    .hint = L'effort à fournir pour le résultat ; vos propres valeurs ci-dessous priment
    .image = Un sticker statique ne reprend du préréglage que les réglages d'image, c'est-à-dire la mise au carré et la mise à l'échelle : il est enregistré sans perte si cela tient, sinon à la meilleure qualité qui tient encore
    .broken = Le fichier de ce préréglage est endommagé
    .missing = Le préréglage « { $name } » n'existe plus : choisissez-en un autre
manage-presets = Gérer…
    .hint = Renommer, décrire ou supprimer vos préréglages
preset-fast = Rapide
    .webm = Ajuste seulement le débit, avec l'encodeur rapide : quelques secondes, mais plus flou
    .tgs = Moins d'une seconde, quelques pour cent plus lourd
preset-balanced = Équilibré
    .webm = Ajuste le débit et essaie des fréquences d'images plus basses quand elles pourraient donner un meilleur rendu
    .tgs = Entre Rapide et Meilleur
preset-best = Meilleur
    .webm = Comme Équilibré, avec l'encodeur le plus lent : plusieurs minutes pour les longues vidéos
    .tgs = Quelques secondes, le fichier le plus léger
preset-described = { $name } : { $description }


## Settings: the picture

picture = Image
picture-size = Taille
once-read = Affiché une fois la source lue
crop = Recadrage
    .hint = La partie de la source à utiliser, en pixels. Faites glisser sur l'aperçu pour la modifier
margin-top = Marge du haut : pixels retirés en haut
margin-left = Marge de gauche : pixels retirés à gauche
margin-right = Marge de droite : pixels retirés à droite
margin-bottom = Marge du bas : pixels retirés en bas
width = Largeur
height = Hauteur
crop-keys = Recadrage : les flèches déplacent le cadre, Ctrl+flèches le redimensionnent ; avec Maj, de dix pixels
whole-picture = Image entière
    .hint = Utiliser toute l'image
    .exact = Utiliser toute l'image, ou ce que l'échelle fixe en prend depuis le coin supérieur gauche
middle = Centre
    .hint = Utiliser ce que l'échelle fixe prend au centre
aspect-ratio = Proportions
    .hint = Le rapport largeur/hauteur que garde le cadre quand vous le faites glisser
    .square = Un TGS doit être carré, et « { square } » est sur « { square-off } », qui garde la forme du cadre : choisir ceci le rend de nouveau carré
shape-free = Libres
    .hint = N'importe quelle forme
shape-preserve = D'origine
    .hint = La forme de la source
shape-custom = Personnalisées
    .hint = Une forme saisie : en rapport, ou en taille du résultat
    .pick = Choisissez « { shape-custom } » pour saisir une forme
ratio-result = Résultat
    .fixed = Le résultat est mis au carré ou a des marges : sa taille ne suit pas le cadre
    .width = Largeur du résultat
    .height = Hauteur du résultat
square = Rendre carré
    .hint = Comment le résultat devient carré
    .tgs = Comment le pixel art devient carré (comme doit l'être tout TGS)
square-off = Non
    .hint = Le résultat garde la forme du cadre
    .emoji = Un émoji doit être carré
    .tgs = Un TGS doit être carré, et le cadre qu'il garde aussi : choisir ceci règle « { aspect-ratio } » sur 1:1
    .kept = Le pixel art garde la forme du cadre, que « { aspect-ratio } » garde carré (comme doit l'être tout TGS)
square-pad = Avec des marges
    .hint = Des marges transparentes complètent le carré
    .tgs = Des marges transparentes complètent le carré
square-crop = En recadrant
    .hint = Le centre remplit le carré et le reste est coupé : l'aperçu l'assombrit
    .tgs = Le centre remplit le carré et le reste est coupé
square-stretch = En étirant
    .hint = L'image est étirée jusqu'au carré
    .tgs = Le pixel art est étiré jusqu'au carré (ses pixels ne sont plus carrés)
square-exact = L'échelle entière fixe garde toute l'image : ajoutez plutôt des marges
scaling = Mise à l'échelle
    .hint = Comment les pixels de la source deviennent ceux du résultat
scaling-auto = Automatique
    .hint = Pixels nets pour le pixel art agrandi deux fois ou plus, lisse sinon
scaling-smooth = Lisse
    .hint = Les pixels voisins se mélangent
scaling-sharp = Pixels nets
    .hint = Chaque pixel devient un bloc, et les blocs ne se mélangent qu'à leurs bords
scaling-integer = Entière
    .hint = Chaque pixel devient le même nombre entier de pixels du résultat, ou des blocs entiers n'en font plus qu'un ; des marges transparentes complètent le reste
integer-scale = Échelle entière
    .hint = Combien de pixels du résultat donne chaque pixel de la source, ou combien de pixels de la source en font un
    .only = Seulement avec la mise à l'échelle « { scaling-integer } »
scale-largest = La plus grande possible
    .hint = Le plus grand multiple entier qui garde le cadre dans le résultat
    .preset = Le préréglage fixe une échelle
scale-fixed = Fixe
    .hint = Cette échelle ; le cadre peut faire au plus ce qu'elle fait tenir dans le résultat
smaller = Plus petite
larger = Plus grande
scale-field = Saisissez 3 pour trois fois plus grand, ou 1/3 pour un tiers
    .pick = Choisissez « { scale-fixed } » pour choisir une échelle
scale-unused = Les échelles entières ne s'appliquent qu'avec la mise à l'échelle « { scaling-integer } »
scale-unsuited = Le cadre ne convient pas encore à cette échelle
scale-makes = { $scale } donne un cadre de { $width } × { $height } px
scale-makes-padded = { $scale } donne un cadre de { $width } × { $height } px ; des marges transparentes complètent le reste des { $box_width } × { $box_height }
scale-each-way = ×{ $across } en largeur et ×{ $down } en hauteur
scale-shrinks = Le cadre est plus grand que le résultat, il est donc réduit en pixels nets : une échelle fixe comme ×1/2 le réduit par blocs entiers
margins = Marges
    .hint = Lignes et colonnes du bord transparentes dans toutes les images de la source
auto-crop = Recadrer automatiquement les marges vides
    .hint = Réduit le pixel art au plus petit rectangle contenant des pixels visibles dans au moins une image
    .after = S'applique après le recadrage ci-dessus
art-pixel = Taille du pixel d'art
    .hint = Combien de pixels de la source occupe un pixel d'art. Les pixels hors de sa grille y sont replacés
art-pixel-detected = Détectée
    .hint = Déduite du pixel art : ses plus petits blocs d'une seule couleur


## Settings: time

time = Temps
used-part = Partie utilisée
    .hint = Où commence et finit la partie utilisée de la source ; la timeline sous l'aperçu la règle aussi
part-from = De
part-to = à
part-first = Sa première image
part-last = Sa dernière image
all-of-input = Toute la source
part-seconds = { $from } à { $to } sur { $length }
part-frame = Image { $number } sur { $count }
part-frames = Images { $first } à { $last } sur { $count }
speed = Vitesse
    .hint = Combien de fois plus vite que la source le résultat est lu
from-preset = Du préréglage
    .hint = La valeur fixée par le préréglage
from-input = De la source
    .speed = Aussi vite que la source
    .rate = La fréquence de la source à cette vitesse, 60 fps au plus
fits-3-s = Tenir en 3 s
    .hint = Aussi vite qu'il faut pour que la partie utilisée tienne en 3 secondes
fit-3-s = Ajuster à 3 s
    .hint = Régler la vitesse pour que la partie utilisée dure 3 secondes
plays = Dure { $played }
plays-cut = Dure { $kept } sur { $used } : le reste est coupé
plays-spoofed = Dure { $played } en entier : Telegram le croit plus court
frame-rate = Fréquence d'images
    .hint = Moins d'images par seconde laissent plus d'octets à chacune ; au-delà de la source, elles ne feraient que se répéter
    .sequence = La vitesse de lecture des images de la séquence
    .sheet = La vitesse de lecture des images d'une séquence ou d'une feuille de sprites
default-rate = Par défaut
    .hint = { $fps } fps, car ces images n'indiquent pas leur vitesse de lecture
over-3-s = Falsifier la durée
    .hint = Telegram refuse les stickers qui annoncent plus de 3 secondes
spoof-auto = Si plus long
    .hint = Les stickers de plus de 3 secondes reçoivent une durée courte inscrite dans leur fichier : Telegram les accepte et les lit en entier
spoof-always = Toujours
    .hint = Même les stickers courts reçoivent cette durée courte
spoof-never = Jamais : couper à 3 s
    .hint = Ne falsifie pas les métadonnées et coupe la vidéo à 3 secondes
spoof-to = Durée annoncée
    .hint = La durée inscrite dans les métadonnées, que lit Telegram
    .honest = Rien n'est falsifié avec « { spoof-never } »
default-duration = Par défaut
    .hint = Une durée courte que Telegram accepte
frame = Image utilisée
frame-at = Image à
    .hint = L'instant de l'image dont est fait le sticker ; la timeline sous l'aperçu la choisit aussi


## Settings: size and quality of a WebM

size-and-quality = Taille et qualité
fitting = Tenir sous la limite
    .hint = Ce que tgradish ajuste pour arriver juste sous la limite de taille du fichier
    .lossless = L'encodage sans perte n'a ni débit ni qualité à ajuster : choisir ceci décoche « { lossless-encoding } »
fit-auto = Meilleur rendu
    .hint = Ajuste le débit et, si rien ne tient ou si moins d'images, avec plus de bits chacune, pourraient donner un meilleur rendu, essaie des fréquences d'images plus basses et garde ce qui ressemble le plus à la source
fit-bitrate = Débit
    .hint = Ajuste le débit à la fréquence d'images choisie
fit-crf = Qualité
    .hint = Ajuste la valeur de qualité constante (CRF)
fit-fps = Fréquence d'images
    .hint = Ajuste la fréquence d'images à qualité constante
fit-length = Durée
    .hint = Ajuste la durée à qualité constante, en coupant la fin
fit-off = Non
    .hint = Encode une seule fois avec les valeurs ci-dessous, même si le fichier est trop lourd
encoder = Effort d'encodage
    .hint = Un encodage plus rapide donne un moins bon rendu à taille égale
encoder-fast = Rapide
    .hint = Quelques secondes, mais visiblement moins bon à taille égale
encoder-balanced = Équilibré
encoder-best = Meilleur
    .hint = Lent : plusieurs minutes pour les longues vidéos
fitting-off = Avec « { fit-off } », un seul encodage
attempts = Encoder jusqu'à
    .hint = Le nombre maximal d'encodages pour tenir sous la limite
attempts-default = Par défaut
    .hint = Assez pour arriver à quelques pour cent de la limite de taille avec la plupart des sources
attempts-times = fois
search-within = Chercher de
    .hint = La plage explorée pour tenir sous la limite, dans l'unité de la valeur ajustée
    .low = Minimum
    .high = Maximum
search-auto = Automatique
    .hint = Aussi large qu'il convient à la méthode choisie et à la source
range-to = à
crf = Qualité (CRF)
    .hint = Qualité constante, de 0, la meilleure, à 63
    .lossless = L'encodage sans perte n'a pas de valeur de qualité
    .fitted = L'ajustement trouve la qualité
    .bitrate = L'ajustement trouve un débit, pas une qualité
crf-default = Par défaut
    .hint = Une bonne qualité aux tailles des stickers
lossless = Sans perte
lossless-encoding = Encodage sans perte
    .hint = Léger seulement pour les images minuscules ou fixes
    .switches = Léger seulement pour les images minuscules ou fixes. L'encodage sans perte n'a ni débit ni qualité à ajuster : cocher ceci fait passer l'ajustement sur « { fit-fps } »
bitrate = Débit
    .hint = Utilisé sauf si seule une qualité est fixée
    .fitted = L'ajustement trouve le débit
    .constant = Encodé à qualité constante, pas à un débit donné
bitrate-auto = Selon la limite de taille
    .hint = Autant que la durée le permet


## Settings: size and quality of a TGS animation

tgs-encoder = Effort d'encodage
    .hint = L'effort mis à chercher un encodage compact
tgs-fast = Rapide
    .hint = Moins d'une seconde, quelques pour cent plus lourd
tgs-balanced = Équilibré
tgs-best = Meilleur
    .hint = Quelques secondes, le fichier le plus léger
too-large = S'il ne tient pas
    .hint = Ce qui se passe quand le pixel art ne tient pas tel quel dans 64 KiB
never-change = Ne jamais modifier le pixel art ; le signaler plutôt
    .set = Le pixel art n'est jamais modifié, comme réglé ci-dessus
give-up = Sacrifier d'abord
    .hint = Quand le pixel art doit changer pour tenir : son mouvement ou ses détails
give-up-either = Au choix
    .hint = Ce qui modifie le moins le pixel art pour les octets gagnés
give-up-motion = Mouvement
    .hint = Les images fusionnent et disparaissent d'abord : tous les détails, mouvement plus saccadé
give-up-detail = Détails
    .hint = L'image s'appauvrit d'abord : mouvement fluide, moins de détails
ways-to-fit = Moyens de le faire tenir
    .hint = Ce que l'ajustement peut faire au pixel art pour qu'il tienne dans 64 KiB, du moins visible au plus visible
reduce-snap = Aligner sur la grille de pixels
    .hint = Replace les pixels égarés sur la grille des pixels d'art
reduce-colours = Fusionner les couleurs proches
    .hint = Les couleurs presque identiques n'en font plus qu'une
reduce-specks = Supprimer les points isolés
    .hint = Les pixels seuls qui diffèrent de tous leurs voisins
reduce-frames = Fusionner les images proches
    .hint = Les images presque identiques n'en font plus qu'une
reduce-drop = Supprimer des images
    .hint = Affiche une image sur deux, ou moins
reduce-resolution = Baisser la résolution
    .hint = Des pixels d'art moins nombreux et plus gros
reading-art = Lecture du pixel art
sprite-sheet = Feuille de sprites
    .hint = Des images disposées dans une grille de cellules égales, ligne par ligne
read-as-sheet = Lire comme une feuille de sprites
sheet-refused = Ce fichier contient déjà { $frames } images : une feuille de sprites est une seule image
    .converted = Lu tel quel, pas comme une feuille de sprites : il contient déjà { $frames } images
sheet-cells = Cellules
    .columns = Colonnes
    .rows = Lignes
sheet-frames = Images
sheet-all = Toutes
    .hint = Toutes les cellules sauf les transparentes à la fin


## Settings: an Aseprite file's tags and layers

aseprite-file = Fichier Aseprite
unreadable-choices = Illisible avec l'étiquette et les calques choisis
read-all = Tout lire
tag = Étiquette
    .hint = Choisit les images d'une étiquette : de son début à sa fin, ou dans son propre ordre si elle se lit à l'envers ou en aller-retour
all-frames = Toutes les images
    .hint = Toutes les images du fichier
tag-frame = Image { $number }
tag-frames = Images { $from } à { $to }
tag-reverse = { $frames }, à l'envers
tag-ping-pong = { $frames }, aller-retour
tag-pong-ping = { $frames }, retour-aller
layers = Calques
    .hint = Les calques visibles, celui du dessus en premier ; un groupe masqué masque ses calques
    .group-hidden = Son groupe est masqué
    .file = Revenir aux calques affichés par le fichier


## Settings: encoder options, folded away under the rest

encoder-options = Options de l'encodeur
vp9-options = Options de l'encodeur par nom, comme tune-content = screen pour les aplats. ffmpeg -h encoder=libvpx-vp9 les liste
    .name = Par exemple : tune-content
    .value = Par exemple : screen
add-option = Ajouter une option
examples = Exemples :
example-tune-content = Aplats et captures d'écran : bords plus nets
example-aq-mode = Dépense les bits là où l'image est complexe
example-sharpness = Garde plus de détails fins, au prix d'un peu de douceur
example-arnr-strength = Moins de réduction du bruit entre les images
example-g = Une image clé au moins toutes les 60 images
example-qmax = Jamais pire que la qualité 50, nulle part
ffmpeg-arguments = Arguments ffmpeg
    .hint = Arguments bruts ajoutés avant la sortie, découpés comme le ferait un shell
    .placeholder = Par exemple : -qmin 4
    .unfinished = Un guillemet ou une barre oblique inverse reste ouvert : les arguments restent tels qu'à leur dernière lecture jusqu'à sa fermeture
    .system = Seulement avec le ffmpeg du système, à choisir dans « { settings } »


## What keeps a file from being converted as it is, and the ways out

blocked = Conversion impossible pour l'instant : { $reason }
    .heading = Conversion impossible pour l'instant
blocked-crop = le cadre sort de l'image
blocked-sheet = les cellules de la feuille de sprites ne divisent pas l'image
blocked-tag = le fichier n'a pas d'étiquette « { $tag } »
blocked-layers = { $count ->
    [1] le fichier n'a pas de calque { $layers }
   *[other] le fichier n'a pas de calques { $layers }
}
blocked-ffmpeg = ffmpeg est introuvable
crop-outside = Le cadre, { $width } × { $height } depuis { $left }, { $top }, sort de cette image de { $across } × { $down }
use-whole = Utiliser l'image entière
    .hint = Ne rien recadrer, garder toute l'image
sheet-uneven = L'image, { $width } × { $height }, ne se divise pas en { $columns } × { $rows } cellules égales
sheet-nearest = La plus proche qui divise : { $columns } × { $rows }
    .hint = La disposition la plus proche de celle-ci dont les cellules divisent l'image
tag-missing = Ce fichier n'a pas d'étiquette « { $tag } »
layers-missing = { $count ->
    [1] Ce fichier n'a pas de calque { $layers }
   *[other] Ce fichier n'a pas de calques { $layers }
}
layers-as-file = Afficher les calques comme dans le fichier
    .hint = Afficher et masquer les calques comme le fait le fichier
needs-ffmpeg = Ceci nécessite ffmpeg, qui est introuvable
    .hint = Installez ffmpeg depuis les paquets de votre système ou depuis ffmpeg.org, ou indiquez où il se trouve. Les fichiers sont relus dès qu'il est trouvé
choose-ffmpeg = Choisir ffmpeg…
    .hint = Choisir le programme ffmpeg, ou un dossier contenant ffmpeg et ffprobe
row-attention = à vérifier
    .hint = Quelque chose empêche sa conversion : voir ses paramètres
row-needs-ffmpeg = ffmpeg requis
    .hint = ffmpeg est nécessaire pour le lire ou créer son résultat, et il est introuvable
need-attention = { $count } à vérifier


## What converting will do, beside the preview, step by step

plan-video = Traitement de la vidéo :
plan-picture = Traitement de l'image :
plan-animation = Traitement de l'animation :
plan-art = Traitement du pixel art :
plan-video-frame = Traitement d'une image de la vidéo :
plan-animation-frame = Traitement d'une image de l'animation :
plan-art-frame = Traitement d'une image du pixel art :
plan-tag = Lecture selon son étiquette « { $tag } »
plan-tag-reverse = Lecture selon son étiquette « { $tag } », à l'envers
plan-tag-ping-pong = Lecture selon son étiquette « { $tag } », en aller-retour
plan-tag-pong-ping = Lecture selon son étiquette « { $tag } », en retour-aller
plan-cropped = Recadrage en { $width } × { $height }
plan-frame = Prise de l'image à { $time }
plan-scaled = Mise à l'échelle en { $width } × { $height }
plan-stretched-across = Étirement ×{ $factor } en largeur
plan-stretched-down = Étirement ×{ $factor } en hauteur
plan-middle-cut = Découpe du centre en { $width } × { $height }
plan-padded = Ajout de marges jusqu'à { $width } × { $height }
plan-trimmed = Rognage aux pixels visibles
plan-grid = Alignement des pixels sur une grille de { $scale } × { $scale }
plan-square-crop = Recadrage en carré
plan-square-stretch = Étirement en carré
plan-square-pad = Ajout de marges jusqu'au carré
plan-cut = Découpe de { $from } à { $to } sur { $length }
plan-faster = Accélération ×{ $factor }
plan-slower = Ralentissement ×{ $factor }
plan-cut-3-s = Coupe à 3 s sur { $played }
plan-spoofed = Durée falsifiée, pour lire les { $played } en entier
plan-spoofed-anyway = Durée falsifiée, bien que { $played } soit assez court
plan-saved = Enregistrement sous { $name }


## Settings: the same conversion as commands, folded away under the rest

commands = Commandes
copy = Copier
commands-posix = La même conversion en ligne de commande, pour un shell POSIX :
commands-powershell = La même conversion en ligne de commande, pour PowerShell :
pasted-temporary = Une image collée est conservée dans un dossier temporaire, supprimé à la fermeture de tgradish
ffmpeg-commands = Les commandes ffmpeg du premier encodage ; l'ajustement modifie le débit, et parfois la fréquence d'images, entre les encodages :
    .builtin = Ce que fait le ffmpeg intégré pour le premier encodage, sous forme de commandes pour le ffmpeg du système ; l'ajustement modifie le débit, et parfois la fréquence d'images, entre les encodages :
no-ffmpeg-commands = Pas de commandes ffmpeg : { $why }
    .unread = la source n'est pas encore lue
    .art = ffmpeg lit un PNG que chaque conversion dessine à partir du pixel art


## The Settings dialog

save = Enregistrer
saved-to = Enregistré dans { $path }
    .kept = Enregistré dans { $path } ; le fichier illisible est conservé sous le nom { $kept }
not-saved = Enregistrement impossible : { $error }
no-home = Aucun dossier personnel où conserver les paramètres
settings-note = Les modifications s'appliquent et sont conservées au fur et à mesure
reset-settings = Réinitialiser
    .hint = Remettre chaque paramètre ici tel que tgradish l'avait au départ
ffmpeg-given = Cette session utilise le ffmpeg indiqué au lancement de tgradish ; ces paramètres s'appliquent quand il est lancé sans
ffmpeg-use = Utiliser
    .hint = Les vidéos sont lues, et les stickers WebM créés, avec ffmpeg
ffmpeg-auto = Automatique
    .hint = Le ffmpeg intégré si cette version en a un, sinon celui du système
ffmpeg-builtin = Intégré
    .hint = Le ffmpeg inclus dans tgradish
    .missing = Cette version de tgradish n'inclut pas ffmpeg
ffmpeg-system = Du système
    .hint = ffmpeg et ffprobe du PATH ; permet les arguments ffmpeg bruts
ffmpeg-program = Programme
    .hint = Un exécutable ffmpeg, ou un dossier contenant ffmpeg et ffprobe ; remplace le choix ci-dessus
    .missing = Il n'y a rien à cet emplacement : le programme utilisé reste le même
ffmpeg-found = { $path }, trouvé dans le PATH
for-example = Par exemple : { $example }
choose = Choisir…
check = Vérifier
    .hint = Vérifier si ce ffmpeg peut créer des stickers
checking-ffmpeg = Vérification…
ffmpeg-ready = ffmpeg { $version } avec VP9 : prêt
ffmpeg-no-vp9 = ffmpeg { $version } ne peut pas encoder en VP9 avec libvpx
converting-settings = Conversion
threads = Threads
    .hint = Combien la conversion en utilise à la fois, au plus ; moins en laisse davantage au reste de l'ordinateur, mais prend plus de temps
threads-auto = Automatique
    .hint = Autant que chaque étape de la conversion peut en utiliser
results = Résultats
save-results = Les enregistrer
next-to-input = À côté de chaque source
in-a-folder = Dans un dossier
    .unchosen = Tant qu'aucun dossier n'est choisi, les résultats sont enregistrés à côté de chaque source
    .relative = Saisissez le chemin complet, depuis la racine, pour changer de dossier
    .file = C'est un fichier, pas un dossier : le dossier utilisé reste le même
existing-files = Fichiers existants
    .hint = Les résultats créés par tgradish sont toujours remplacés quand ils sont recréés
replace-in-the-way = Remplacer tout fichier qui occupe la place
interface = Interface
language = Langue
    .hint = La langue de l'interface ; la ligne de commande reste en anglais
    .variable = Choisie par la variable d'environnement { $variable }
as-the-system = Comme le système
as-the-system-language = Comme le système : { $language }
theme = Thème
    .hint = Le bouton à côté de ⚙ bascule aussi entre clair et sombre
theme-light = Clair
theme-dark = Sombre
accent = Couleur d'accentuation
    .hint = Ce qui marque l'élément choisi, survolé ou actif
scale = Échelle
    .hint = La taille du texte et des commandes
    .keys = { $larger } et { $smaller } la changent aussi ; { $reset } revient à 100 %
scale-smaller = Plus petite
scale-larger = Plus grande
scale-reset = Revenir à 100 %
scale-percent = { $percent } %
accent-system = { as-the-system }
    .hint = La couleur d'accentuation du bureau, ou le bleu de Breeze s'il n'en a pas
accent-pink = Rose
accent-red = Rouge
accent-orange = Orange
accent-yellow = Jaune
accent-green = Vert
accent-teal = Bleu canard
accent-blue = Bleu
accent-purple = Pourpre
accent-violet = Violet
accent-grey = Gris
accent-own = Couleur personnalisée
    .chosen = Couleur personnalisée, { $colour }
accent-shade = Nuance
    .value = Saturation { $saturation } %, luminosité { $value } %
accent-hue = Teinte
accent-hex = Hex
    .wrong = Écrivez-la sous la forme # suivi de six chiffres hexadécimaux, comme #3daee9
files = Fichiers
reopen-files = Rouvrir les fichiers de la dernière fois, avec leurs paramètres
motion = Animations
    .hint = Si le système demande moins d'animations, rien ne bouge de toute façon
motion-on = Adoucir le défilement, faire apparaître et glisser ce qui change
new-files = Nouveaux fichiers
preset-for-webm = Préréglage WebM
preset-for-tgs = Préréglage TGS


## The result, beside the preview

last-conversion = Dernière conversion
path-in-the-way = { $path } occupe la place
stopped-early = Arrêté avant la fin
result = Résultat
    .stale = Résultat, obsolète
result-copy = Copiez-le ({ $keys }) pour le coller dans une discussion Telegram
result-webm = { $width } × { $height } px, { $fps } fps, { $length }
    .spoofed = { $width } × { $height } px, { $fps } fps, { $length }, durée falsifiée
changed-to-fit = Modifié pour tenir : détails ci-dessous
replaced-earlier = A remplacé un résultat précédent créé par tgradish
converting = Conversion
stage-reading = Lecture du pixel art
stage-drawing = Conversion en vectoriel
stage-fitting-steps = { $count ->
    [one] Ajustement à 64 KiB : { $count } étape, environ { $size }
    [many] Ajustement à 64 KiB : { $count } étapes, environ { $size }
   *[other] Ajustement à 64 KiB : { $count } étapes, environ { $size }
}
stage-fitting-from = Ajustement à 64 KiB depuis { $size }
stage-compressing = Compression
    .hopeless = Compression rapide : il ne peut pas tenir
    .missed = Compressé → { $size }, trop lourd
    .missed-far = Compression : encore bien trop lourd
retry = Essai { $number } : objectif { $aim }
retry-done = Essai { $number } : objectif { $aim } → { $size }, trop lourd
    .far = Essai { $number } : objectif { $aim } → encore bien trop lourd
starting = Démarrage
encode = Encodage { $number } : { $fps } fps, { $rate }
encode-of = Encodage { $number } sur { $most } max : { $fps } fps, { $rate }
encode-done = Encodage { $number } : { $fps } fps, { $rate } → { $size }, tient
    .too-large = Encodage { $number } : { $fps } fps, { $rate } → { $size }, trop lourd
rate-quality = qualité { $crf }
rate-lossless = sans perte
encodes-up-to = { $count ->
    [one] Au plus { $count } encodage, jusqu'à en obtenir un juste sous la limite de taille
    [many] Au plus { $count } encodages, jusqu'à en obtenir un juste sous la limite de taille
   *[other] Au plus { $count } encodages, jusqu'à en obtenir un juste sous la limite de taille
}
art-size = { $width } × { $height } pixels d'art
    .scaled = { $width } × { $height } pixels d'art, chacun de { $scale } × { $scale } pixels de la source
art-summary = { $size } ; { $colours }, { $frames }
art-tempo = Vitesse ×{ $tempo } par rapport à la source
art-off-grid = Il ressemble surtout à du pixel art ×{ $scale } avec quelques pixels hors de la grille ; une taille de pixel d'art de { $scale } les aligne
art-shapes = { $layers } avec { $rectangles }, { $size } décompressé
art-before-fitting = Environ { $size } tel quel ; modifié pour tenir :
art-step = { $change }, environ { $size } ensuite
step-snapped = pixels alignés sur une grille de { $scale }
step-colours = couleurs à moins de { $distance } % fusionnées
step-frames = images changeant de moins de { $changed } % fusionnées
step-dropped = { $share } % des images supprimées
step-specks = pixels isolés supprimés
step-resolution = résolution réduite à { $factor } %
column-number = Chaque encodage dans l'ordre ; ✔ celui conservé
column-fps = Images par seconde
column-rate = Débit, ou qualité de l'encodage
column-size = taille
    .hint = Part de la limite de taille, en vert si ça tient
column-alike = fidélité
    .hint = Ressemblance avec la source, mesurée par SSIM (similarité structurelle)
rate-quality-short = q { $crf }
decided-in-band = Arrêt : l'encodage { $number } utilise { $share } de la limite, au moins les { $least } visés
decided-alike-enough = Arrêt : l'encodage { $number } ressemble déjà à { $alike } à la source, plus de bits ne feraient que l'alourdir
decided-alike-unchanged = Arrêt : l'encodage { $number } ressemble à { $alike } à la source, à peine plus que l'encodage { $than } avec moins de bits
decided-little-to-gain = Arrêt : l'encodage { $number } utilise { $share } de la limite, et un autre n'en utiliserait qu'environ { $predicted }
decided-too-large-at-least = Arrêt : même le minimum de la plage, à l'encodage { $number }, utilise { $share } de la limite
decided-nothing-left = Arrêt : plus aucun réglage ne vaut la peine d'être essayé
decided-out-of-attempts = { $count ->
    [one] Arrêt : { $count } encodage autorisé, déjà effectué
    [many] Arrêt : les { $count } encodages autorisés sont effectués
   *[other] Arrêt : les { $count } encodages autorisés sont effectués
}
decided-lowers-to-fit = Essai à { $fps } fps : rien ne tenait à { $from } fps
decided-lowers-for-looks = Essai à { $fps } fps : à { $from } fps, l'encodage { $number } ne ressemble qu'à { $alike } à la source
decided-settled = Arrêt anticipé : encodage { $number } conservé
decided-lower-no-better = Arrêt : à { $fps } fps, la ressemblance avec la source est de { $alike }, pas mieux que les { $kept_alike } de l'encodage { $kept } à { $kept_fps } fps
issue-not-webm = Son conteneur est { $container }, pas WebM
issue-no-video = Il n'a pas de piste vidéo
issue-not-vp9 = Son codec est { $codec }, pas VP9
issue-audio = Il a une piste audio
issue-size = Sa taille est de { $width } × { $height } px, alors que Telegram attend { $side } × { $side } px
    .sticker = Sa taille est de { $width } × { $height } px, alors que Telegram attend un côté de { $side } px et l'autre au plus autant
issue-too-big = Il pèse { $size }, au-delà de la limite de { $limit }
issue-too-long = Ses métadonnées indiquent une durée de { $seconds }, au-delà de la limite de { $limit }
issue-too-many-fps = Sa fréquence d'images est de { $fps } fps, au-delà de la limite de { $limit } fps
issue-animated = Il est animé, alors qu'un sticker statique est une seule image


## Inspecting a sticker

inspect-another = Inspecter un autre…
inspect-webm-sticker = Un sticker vidéo
inspect-webm-emoji = Un émoji vidéo
inspect-image-sticker = Un sticker statique
inspect-image-emoji = Un émoji statique
inspect-tgs = Une animation vectorielle, { $width } × { $height } à { $fps } fps
inspect-unknown = Un type de sticker que cette version ne sait pas afficher
inspect-size = Taille
    .hint = Telegram refuse les fichiers plus lourds
    .tgs = Telegram refuse les stickers plus lourds
inspect-duration = Durée
    .hint = Telegram lit la durée dans les métadonnées ; la falsifier la raccourcit
    .spoofed = { $seconds } s dans les métadonnées, falsifiée
    .no-header = { $seconds }, aucune dans les métadonnées
inspect-plays = Durée réelle
    .hint = La durée réelle de la vidéo
inspect-frame-rate = Fréquence d'images
    .value = { $fps } fps sur { $most }, { $frames }
inspect-picture = Image
    .none = aucune
inspect-video = { $width } × { $height } px, { $codec }
    .alpha = { $width } × { $height } px, { $codec }, avec transparence
inspect-webp = { $width } × { $height } px, WebP, sans perte
    .alpha = { $width } × { $height } px, WebP, sans perte, avec transparence
    .lossy = { $width } × { $height } px, WebP, avec perte
    .lossy-alpha = { $width } × { $height } px, WebP, avec perte, avec transparence
inspect-audio = Audio
inspect-title = Titre
inspect-made-with = Créé avec
    .hint = Les programmes qui ont écrit le fichier, d'après lui
    .image = Le programme indiqué par ses métadonnées
    .unknown = inconnu
inspect-note = Note
    .hint = Texte dans le remplissage du fichier, ignoré par les lecteurs
inspect-name = Nom
    .hint = Le nom propre de l'animation, que Telegram n'affiche pas
inspect-length = Durée
    .hint = Les stickers animés durent au plus 3 secondes
    .value = { $seconds } sur 3.00 s, { $frames }
inspect-cost = Coût de rendu
    .hint = Les formes plus 9 par calque, comme les compte Telegram
inspect-layers = Calques
inspect-shapes = Formes max par calque
inspect-refuses-more = Telegram refuse davantage
inspect-points = Points max par remplissage
    .hint = Les points de tracé qu'un remplissage peint ; Telegram refuse davantage
inspect-unpacked = Décompressé
    .hint = Telegram Desktop ne lit pas les animations plus lourdes
inspect-features = Fonctionnalités
    .hint = Fonctionnalités Lottie au-delà des formes et des transformations


## What went wrong, as converting or reading a file reports it

crashed = tgradish a planté : { $details }
ffmpeg-not-found = ffmpeg est introuvable : { $details }
ffmpeg-not-on-path = ffmpeg et ffprobe ne sont pas dans le PATH : installez-les, ou choisissez un autre ffmpeg dans « { settings } »
ffmpeg-args-need-program = Les arguments ffmpeg nécessitent le ffmpeg du système, à choisir dans « { settings } » ; les options de l'encodeur fonctionnent avec les deux
program-failed = { $program } a échoué ({ $status }) :
unreadable-file = Impossible de lire { $name } : { $details }
no-video = { $name } ne contient pas de vidéo
no-such-file = { $name } n'existe plus
not-webm = { $name } n'est pas un fichier WebM
not-sticker = { $name } n'est pas un sticker : un fichier WebM, WebP ou .tgs
no-frame-at = { $name } n'a pas d'image à { $seconds }
nothing-fits = Rien ne tient dans la limite de { $limit } : le plus petit encodage faisait { $smallest }. Une partie utilisée plus courte, une fréquence d'images plus basse ou une plage de recherche plus large peuvent aider
clipboard-failed = Impossible de coller depuis le presse-papiers : { $details }
output-is-input = Le résultat écraserait { $name }, à partir duquel il est créé
nothing-to-convert = Aucune image à convertir


## Saving and managing presets

save-preset-title = { -save-settings }
save-preset-note = Les paramètres de ce fichier, comme point de départ pour n'importe quel fichier, sauf ceux qui ne conviennent qu'à lui : sticker ou émoji, recadrage, partie utilisée, étiquette, calques et feuille de sprites
preset-name = Nom
    .placeholder = mes stickers
preset-description = Description
    .placeholder = facultative
    .none = sans description
preset-exists = Un préréglage de ce nom existe déjà : l'enregistrer le remplace
replace = Remplacer
name-it-first = Nommez-le d'abord
cancel = Annuler
preset-name-length = Le nom d'un préréglage compte de 1 à { $most } caractères et ne commence pas par une espace
preset-name-characters = Le nom d'un préréglage ne contient que des lettres, des chiffres, des espaces, - et _
preset-name-builtin = « { $name } » est le nom d'un préréglage intégré
preset-name-taken = Un autre préréglage s'appelle « { $name } »
preset-extended = « { $other } » en dérive : modifiez d'abord ce préréglage
your-presets = Vos préréglages
your-presets-note = Les modifications sont conservées au fur et à mesure. « { preset-fast } », « { preset-balanced } » et « { preset-best } » sont intégrés et ne changent pas
no-presets = Aucun pour l'instant : « { -save-settings } », dans la barre sous les paramètres
preset-broken = Son fichier est illisible
delete-it = Le supprimer
deleted = « { $name } » supprimé
undo = Annuler
    .hint = Le remettre
not-put-back = Impossible de remettre le préréglage « { $name } »


## About and licences

about-title = À propos et licences
about-summary = Des stickers et émojis Telegram à partir de vidéos, d'images ou de pixel art. Licence MIT ;
source-code = code source
send-feedback = Envoyer un avis sur Telegram
about-notices = tgradish est aussi construit à partir du travail d'autres personnes. Leurs licences demandent que ces mentions accompagnent chaque copie, le programme les inclut donc :
notice-licence = { $name } : licence
notice-named-licence = { $name } : { $licence }
notice-licence-notes = { $name } : notes de licence
notice-patent-grant = { $name } : concession de brevets
notice-sources = { $name } : ce qui est intégré, et ses sources
notice-font = { $subsets ->
    [1] Police : { $name }, un sous-ensemble
   *[other] Police : { $name }, des sous-ensembles
}
notice-crates = Crates Rust
notice-crates-licence = { $licence } : { $crates }
    .more = { $licence } : { $crates } et { $more } de plus
ffmpeg-sources = Les sources exactes du ffmpeg intégré, avec cette version


## The dialog of keyboard shortcuts, opened from the menu or by ? or F1

key-ctrl = Ctrl
key-cmd = Cmd
key-alt = Alt
key-shift = Maj
key-enter = Entrée
key-space = Espace
key-delete = Suppr
key-backspace = Retour arrière
key-home = Début
key-end = Fin
key-escape = Échap
key-left = ←
key-right = →
key-up = ↑
key-down = ↓
shortcuts-files = Fichiers
shortcuts-playing = L'aperçu et sa timeline
shortcuts-general = Général
shortcuts-mouse = La souris, sur l'aperçu et la timeline
shortcut-add-files = Ajouter des fichiers
shortcut-paste = Ajouter les fichiers ou l'image du presse-papiers
shortcut-select = Sélectionner le fichier au-dessus ou en dessous
shortcut-convert = Convertir le fichier sélectionné
shortcut-convert-all = Tout convertir : chaque fichier sans résultat à jour
shortcut-keep-best = Garder le meilleur encodage jusqu'ici, pendant l'ajustement d'un WebM
shortcut-copy-result = Copier le résultat du fichier sélectionné, pour le coller dans Telegram
shortcut-remove = Retirer le fichier sélectionné, ou celui dont la ligne est active
shortcut-undo = Annuler une modification du fichier sélectionné, ou remettre des fichiers retirés
shortcut-redo = Rétablir une modification annulée
shortcut-play = Lecture ou pause
shortcut-step = Une image en arrière ou en avant ; avec Maj, dix
shortcut-ends = La première ou la dernière image utilisée
shortcut-cut-here = Commencer ou finir la partie utilisée à l'image affichée
shortcut-move-start = Déplacer le début de la partie utilisée ; avec Maj, de dix images
shortcut-move-end = Déplacer la fin de la partie utilisée ; avec Maj, de dix images
shortcut-settings = Ouvrir les paramètres
shortcut-inspect = Inspecter un sticker
shortcut-scale-up = Tout agrandir
shortcut-scale-down = Tout réduire
shortcut-scale-reset = Revenir à 100 %
shortcut-shortcuts = Cette liste
shortcut-close = Fermer une boîte de dialogue ou un menu
with-keys = { $hint } ({ $keys })
mouse-drag = Glisser
    .does = Recadrer, ou déplacer le cadre ou ses bords
mouse-shape = Maj+glisser un coin
    .does = Garder la forme du cadre, sauf si des proportions sont fixées
mouse-centred = Alt+glisser un bord
    .does = Redimensionner le cadre depuis son centre
mouse-free = Ctrl+glisser
    .does = Recadrer sans aimanter aux bords et au centre de l'image
mouse-cancel = Échap pendant le glissement
    .does = Remettre le cadre comme avant
mouse-crop-keys = Clic sur le cadre, puis flèches
    .does = Le déplacer d'un pixel, de dix avec Maj ; Ctrl+flèches le redimensionnent
mouse-double-click = Double-clic
    .does = Revenir à l'image entière
mouse-wheel = Molette
    .does = Zoomer ou dézoomer
mouse-pan = Glisser avec le bouton droit ou central
    .does = Se déplacer, une fois zoomé
mouse-pan-across = Maj+molette
    .does = Se déplacer horizontalement, une fois zoomé
mouse-slide = Maj+glisser un crochet
    .does = Faire glisser la partie utilisée sur la timeline, en gardant sa durée
