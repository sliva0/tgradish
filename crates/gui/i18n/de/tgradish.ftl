### tgradish's words in German. What each message means and where it is
### shown is said in the comments of the English file, en-GB/tgradish.ftl.

language-name = Deutsch


## Shared by many places

close = Schließen
back-to-preset = Zurück zum Wert der Vorgabe
custom = Manuell
    .hint = „{ custom }“ wählen, um einen Wert festzulegen
webm-sticker = WebM-Sticker
webm-emoji = WebM-Emoji
tgs-sticker = TGS-Sticker
tgs-emoji = TGS-Emoji
image-sticker = Statischer Sticker
image-emoji = Statisches Emoji
count-colours = { $count ->
    [one] { $count } Farbe
   *[other] { $count } Farben
}
count-frames = { $count ->
    [one] { $count } Frame
   *[other] { $count } Frames
}
count-layers = { $count ->
    [one] { $count } Ebene
   *[other] { $count } Ebenen
}
count-rectangles = { $count ->
    [one] { $count } Rechteck
   *[other] { $count } Rechtecke
}
count-tracks = { $count ->
    [one] { $count } Spur
   *[other] { $count } Spuren
}
of-limit = { $used } von { $limit }
telegram-accepts = Telegram sollte es annehmen


## Messages along the bottom

dismiss = Ausblenden
    .all = Alle ausblenden
said-times = ×{ $count }
details = Details
settings-unreadable = Die Einstellungsdatei ist nicht lesbar, daher gelten die Standardwerte
setting-unsaved = Die Änderung konnte nicht in der Einstellungsdatei gespeichert werden
    .unreadable = Änderung nicht gespeichert: Die Einstellungsdatei ist nicht lesbar
settings-file-note = Vielleicht wurde sie von Hand oder mit einer anderen Version von tgradish bearbeitet. Bis sie repariert oder ersetzt ist, lassen sich Änderungen nicht speichern; ihr Inhalt geht nicht verloren.
fresh-settings = Mit neuen Einstellungen beginnen
    .hint = Die aktuellen Einstellungen als neue Einstellungsdatei schreiben; die alte bleibt daneben erhalten
    .saved = Neue Einstellungen gespeichert
    .kept = Die alte Datei liegt daneben als { $name }
    .failed = Neue Einstellungen konnten nicht geschrieben werden
read-again = Erneut versuchen
    .hint = Erneut einlesen, sobald sie repariert ist
    .failed = Immer noch nicht lesbar
settings-read = Die Einstellungsdatei ist wieder lesbar, ihre Einstellungen gelten
copy-path = Pfad kopieren
    .hint = Den Speicherort der Datei kopieren, um sie in einem Editor zu öffnen
    .done = Kopiert
files-removed = { $count ->
    [one] 1 Datei entfernt
   *[other] { $count } Dateien entfernt
}
undo-removing = Rückgängig
    .hint = Die Dateien samt Einstellungen und Ergebnissen zurück an ihren Platz bringen (Strg+Z)
settings-reset = Die Einstellungen sind auf Standard zurückgesetzt
undo-reset = Rückgängig
    .hint = Die vorherigen Einstellungen wiederherstellen
folder-of = Im Ordner anzeigen
    .hint = Den Ordner öffnen, in dem sie liegt
presets-unreadable = Die eigenen Vorgaben konnten nicht gelesen werden, daher sind nur die integrierten verfügbar
files-gone = { $count ->
    [one] { $count } Datei vom letzten Mal ist nicht mehr vorhanden
   *[other] { $count } Dateien vom letzten Mal sind nicht mehr vorhanden
}
session-unreadable = Die Dateien vom letzten Mal konnten nicht gelesen werden, daher ist keine geöffnet; ihre Liste wurde beiseitegelegt
files-unreadable = { $count ->
    [one] { $count } Datei vom letzten Mal konnte nicht gelesen werden und fehlt; die bisherige Liste wurde beiseitegelegt
   *[other] { $count } Dateien vom letzten Mal konnten nicht gelesen werden und fehlen; die bisherige Liste wurde beiseitegelegt
}
session-unsaved = Die geöffneten Dateien konnten nicht für das nächste Mal gespeichert werden
nothing-to-paste = Nichts zum Einfügen
    .why = Die Zwischenablage enthält keine Dateien, Dateipfade oder Bilder
paste-failed = Die Zwischenablage konnte nicht gelesen werden
    .image = Das eingefügte Bild konnte nicht gespeichert werden
pick-failed = Der Dateidialog ist abgestürzt, daher wurde nichts ausgewählt


## Dropping files

drop-to-add = Ablegen, um Dateien hinzuzufügen


## The system's file dialogs: their titles, which say what each is for,
## and the names of the kinds of files they show

dialog-add-files = Dateien zum Konvertieren hinzufügen
dialog-add-folder = Ordner mit Frames hinzufügen
dialog-add-images = Bilder als eine Animation hinzufügen
dialog-inspect = Sticker zum Prüfen auswählen
dialog-ffmpeg = ffmpeg auswählen
dialog-results-folder = Ordner für Ergebnisse auswählen
images-filter = Bilder
stickers-filter = Sticker


## The list of files, on the left

settings = Einstellungen
light-theme = Zum hellen Design wechseln
dark-theme = Zum dunklen Design wechseln
main-menu = Einstellungen und mehr
menu-undo = Rückgängig
    .hint = Macht die letzte Änderung an den Einstellungen der Datei oder das Entfernen von Dateien rückgängig
menu-redo = Wiederherstellen
    .hint = Stellt die zuletzt rückgängig gemachte Änderung wieder her
menu-settings = Einstellungen…
inspect = Sticker prüfen…
    .hint = Eine Datei mit den Limits von Telegram abgleichen
feedback = Feedback senden
    .hint = Dem Autor auf Telegram schreiben: in der App, falls installiert, sonst im Browser
about-and-licences = Info und Lizenzen…
show-input-in-folder = Im Ordner anzeigen
    .pasted = Ein eingefügtes Bild hat keine eigene Datei
shortcuts = Tastenkürzel
add-files = Hinzufügen…
add-more = Weitere Möglichkeiten zum Hinzufügen
add-folder = Ordner mit Frames…
    .hint = Nach Namen sortiert, als eine Animation
add-images = Bilder als eine Animation…
    .hint = Die gewählten Bilder, nach Namen sortiert
add-pasted = Aus der Zwischenablage
    .hint = Kopierte Dateien, Pfade oder ein Bild
convert-all = { $count ->
    [0] Alle konvertieren
   *[other] Alle konvertieren ({ $count })
}
    .hint = Jede Datei ohne aktuelles Ergebnis konvertieren
remove-all = Alle entfernen
converting-batch = Konvertierung { $current } von { $total }
stop-all = Alle stoppen
    .hint = Die Konvertierung stoppen und wartende Dateien aus der Warteschlange nehmen
remove = Entfernen
remove-file = Aus der Liste entfernen
convert = Konvertieren
    .hint = Den Sticker oder das Emoji erstellen
row-convert-again = Veraltet: neu konvertieren
row-next = als Nächstes
row-waiting = wartet, { $ahead } davor
row-stale = { $size } · veraltet
row-failed = fehlgeschlagen
row-exists = eine Datei ist im Weg
row-stopped = gestoppt
drop-here = Videos, GIFs, Bilder oder Aseprite-Dateien hier ablegen
drop-here-note = Zum Konvertieren in Video-, animierte oder statische Sticker und Emojis
paste = Einfügen
name-pasted = eingefügtes Bild
name-frames = { $name } (Frames)
name-and-more = { $name } und { $more } weitere
no-font = { $count ->
    [one] Für { $codes } wurde keine Schriftart gefunden, weder unter denen von tgradish noch unter denen des Systems, daher wird es als Code angezeigt. Die Datei selbst ist in Ordnung
   *[other] Für { $codes } wurde keine Schriftart gefunden, weder unter denen von tgradish noch unter denen des Systems, daher werden sie als Codes angezeigt. Die Datei selbst ist in Ordnung
}


## The bar under the settings of the file picked

stop = Stoppen
stopping = Wird gestoppt…
keep-best = Bestes behalten
    .hint = Keine weiteren Kodierungen versuchen und die bisher beste passende als Ergebnis speichern
    .nothing-fits = Noch passt nichts: Es gibt keine Kodierung zum Behalten
keeping-best = Bestes wird behalten…
cancel-waiting = Abbrechen
next-to-convert = Kommt als Nächstes
waiting-for = Wartet auf { $ahead } weitere
convert-again = Neu konvertieren
    .changed = Veraltet: mit den aktuellen Einstellungen neu erstellen
    .hint = Mit denselben Einstellungen neu erstellen
-save-settings = Aktuelle Einstellungen als Vorgabe speichern
save-preset = { -save-settings }…
    .hint = Als eigene Vorgabe behalten, mit der jede Datei beginnen kann
failed = Fehlgeschlagen: { $error }
    .short = Fehlgeschlagen – siehe Details
in-the-way = Eine Datei, die nicht von tgradish stammt, ist im Weg:
replace-it = Ersetzen
stopped = Gestoppt
show-in-folder = Im Ordner anzeigen
saves-as = Speichert als
    .hint = Wohin Ergebnisse gespeichert werden, wird in „{ settings }“ festgelegt
result-saved-as = Gespeichert als
copy-result = Kopieren
    .hint = Den Sticker als Datei kopieren, um ihn in einen Telegram-Chat einzufügen
    .done = Kopiert
    .failed = Nicht kopiert
replaces-earlier = Ersetzt ein früheres Ergebnis
    .any = Ersetzt die vorhandene Datei


## Reading a file to show it

reading-crashed = tgradish ist beim Lesen abgestürzt
reading-stopped = Lesen abgebrochen
no-frames = ffmpeg hat darin keine Frames gefunden
no-frames-there = ffmpeg hat dort keine Frames gefunden
no-frame-there = ffmpeg hat dort keinen Frame gefunden
art-no-frames = Enthält keine Frames
result-no-frames = Das Ergebnis enthält keine Frames


## The result's small picture over the preview

inset-label = Ergebnis bei { $scale }
    .plain = Ergebnis
    .drag = In eine andere Ecke ziehen
inset-scale = { $percent }%
    .times = { $times }×
inset-smaller = Kleiner
    .same = Kleiner geht es nicht
inset-larger = Größer
    .same = Größer geht es nicht


## Over the preview

show-input = Quelle
show-result = Ergebnis
    .outdated = Ergebnis ⚠
    .hint = Veraltet: Das Ergebnis wurde mit früheren Einstellungen erstellt
convert-first = Erst konvertieren
tgs-unseen = Dieser TGS-Sticker enthält Formen, die tgradish nicht zeichnen kann
result-preview = Ergebnisvorschau
    .hint = Kleine Vorschau neben der Quelle ein-/ausblenden
    .input-only = Wird neben der Quelle angezeigt, nicht neben dem Ergebnis
zoom-out = Herauszoomen
    .hint = Alles zeigen
    .all = Bereits ganz herausgezoomt
input-hint = Ziehen zum Zuschneiden · Mausrad zum Zoomen · rechte Taste zum Verschieben
crop-hint = Zuschnitt oder Kanten ziehen · Doppelklick für alles
result-hint = Mausrad zum Zoomen · rechte Taste zum Verschieben
unreadable = Nicht lesbar: { $error }
reading = Wird gelesen…
preview-divider = Trennlinie unter der Vorschau


## The timeline under the preview

play = Abspielen
    .pause = Pause
play-keys =
    Leertaste — abspielen oder pausieren
    ← / → — einen Frame zurück oder vor
    Umschalt+← / → — zehn Frames zurück oder vor
    Pos1 / Ende — erster oder letzter verwendeter Frame
    {"["} / ] — Abschnitt beim angezeigten Frame beginnen oder enden lassen
    Strg+← / → — Beginn des Abschnitts verschieben
    Alt+← / → — Ende des Abschnitts verschieben
nothing-to-play = Nur ein Frame wird verwendet: Es gibt nichts abzuspielen
in-seconds = Sekunden
    .hint = Zeiten in Sekunden
in-frames = Frames
    .hint = Zeiten als Frame-Nummern, ab 1
    .unknown = Wo die Frames der Quelle liegen, ist nicht bekannt
part-start = Beginn des Abschnitts
    .frame = Beginn des Abschnitts: sein erster Frame
part-end = Ende des Abschnitts
    .frame = Ende des Abschnitts: sein letzter Frame
frame-number = Frame { $number }
limit-mark = Telegrams Limit von 3 Sekunden: Was vom Abschnitt dahinter liegt, wird abgeschnitten
    .spoofed = Telegrams Limit von 3 Sekunden: Was vom Abschnitt dahinter liegt, wird ebenfalls abgespielt, da Telegram eine kürzere Dauer gemeldet wird
    .sped-up = Telegrams Limit von 3 Sekunden: Ein längerer Abschnitt wird schneller abgespielt, damit er hineinpasst
frame-before = Vorheriger Frame (←)
    .first = Das ist der erste Frame
frame-after = Nächster Frame (→)
    .last = Das ist der letzte Frame
frame-picked = Gewählter Frame


## What to make, beside the preview

make = Erstellen
format-video = Video
    .hint = Ein Video-Sticker oder -Emoji: VP9 in WebM, aus jedem Video oder Bild
format-animation = Animiert
    .hint = Ein animierter Sticker oder ein animiertes Emoji: TGS-Datei. Diese Option ist für Pixel-Art gedacht: TGS ist ein Vektorformat und bleibt daher in jeder Größe scharf. Am besten für Custom-Emojis, die nur 100 × 100 px groß sind. Nicht für gewöhnliche Videos oder Fotos: Dafür „{ format-video }“ verwenden
format-image = Statisch
    .hint = Ein statischer Sticker oder ein statisches Emoji: WebP, aus jedem Bild oder einem Frame eines Videos
target-sticker = Sticker
target-emoji = Custom-Emoji
needs-webm-sticker = WebM, 512 px an der längeren Seite, bis 256 KiB
needs-webm-emoji = WebM, 100 × 100 px, bis 64 KiB
needs-tgs = TGS, quadratisch, bis 64 KiB
needs-image-sticker = WebP, 512 px an der längeren Seite, bis 512 KiB
needs-image-emoji = WebP, 100 × 100 px, bis 512 KiB
not-pixel-art = „{ format-animation }“ ist für Pixel-Art gedacht, die in Vektoren umgewandelt wird und so scharf bleibt. Das hier sieht nicht nach Pixel-Art aus: „{ format-video }“ passt besser
preset = Vorgabe
    .hint = Wie viel Aufwand in das Ergebnis fließt; eigene Werte unten haben Vorrang
    .image = Ein statischer Sticker übernimmt von der Vorgabe nur die Bildeinstellungen, also wie er quadratisch gemacht und skaliert wird: Er wird verlustfrei gespeichert, wenn das passt, sonst in der besten Qualität, die noch passt
    .broken = Die Datei dieser Vorgabe ist beschädigt
    .missing = Die Vorgabe „{ $name }“ gibt es nicht mehr: eine andere wählen
manage-presets = Verwalten…
    .hint = Eigene Vorgaben umbenennen, beschreiben oder löschen
preset-fast = Schnell
    .webm = Passt nur die Bitrate an, mit dem schnellen Encoder: dauert Sekunden, ist aber unschärfer
    .tgs = Dauert unter einer Sekunde, einige Prozent größer
preset-balanced = Ausgewogen
    .webm = Passt die Bitrate an und probiert niedrigere Frameraten, wo sie besser aussehen könnten
    .tgs = Zwischen schnell und gründlich
preset-best = Gründlich
    .webm = Wie ausgewogen, mit dem langsamsten Encoder: dauert bei langen Videos Minuten
    .tgs = Dauert einige Sekunden, kleinste Datei
preset-described = { $name }: { $description }


## Settings: the picture

picture = Bild
picture-size = Größe
once-read = Wird angezeigt, sobald die Quelle gelesen ist
crop = Zuschnitt
    .hint = Der verwendete Teil der Quelle, in ihren Pixeln. Zum Ändern in der Vorschau ziehen
margin-top = Oberer Rand: oben abgeschnittene Pixel
margin-left = Linker Rand: links abgeschnittene Pixel
margin-right = Rechter Rand: rechts abgeschnittene Pixel
margin-bottom = Unterer Rand: unten abgeschnittene Pixel
width = Breite
height = Höhe
crop-keys = Zuschnitt: Pfeiltasten verschieben ihn, Strg+Pfeiltasten ändern seine Größe; mit Umschalt um zehn Pixel
whole-picture = Ganzes Bild
    .hint = Das ganze Bild verwenden
    .exact = Das ganze Bild verwenden oder so viel davon, wie der feste Faktor von oben links aus erfasst
middle = Mitte
    .hint = So viel von der Mitte verwenden, wie der feste Faktor erfasst
aspect-ratio = Seitenverhältnis
    .hint = Das Verhältnis von Breite zu Höhe, das der Zuschnitt beim Ziehen beibehält
    .square = TGS muss quadratisch sein, und „{ square }“ steht auf „{ square-off }“, wodurch die Form des Zuschnitts erhalten bleibt: Diese Wahl macht das Ergebnis wieder quadratisch
shape-free = Frei
    .hint = Beliebige Form
shape-preserve = Original
    .hint = Die Form der Quelle
shape-custom = Eigenes
    .hint = Eine eingegebene Form: als Verhältnis oder als Größe des Ergebnisses
    .pick = „{ shape-custom }“ wählen, um eine Form einzugeben
ratio-result = Ergebnis
    .fixed = Das Ergebnis wird quadratisch gemacht oder hat Ränder, daher folgt seine Größe nicht dem Zuschnitt
    .width = Breite des Ergebnisses
    .height = Höhe des Ergebnisses
square = Quadratisch machen
    .hint = Wie das Ergebnis quadratisch wird
    .tgs = Wie die Pixel-Art quadratisch wird (das muss jede TGS-Datei sein)
square-off = Aus
    .hint = Das Ergebnis behält die Form des Zuschnitts
    .emoji = Emojis müssen quadratisch sein
    .tgs = TGS muss quadratisch sein, ebenso der Zuschnitt, dessen Form erhalten bleibt: Diese Wahl setzt „{ aspect-ratio }“ auf 1:1
    .kept = Die Pixel-Art behält die Form des Zuschnitts, die „{ aspect-ratio }“ quadratisch hält (das muss jede TGS-Datei sein)
square-pad = Durch Auffüllen
    .hint = Transparente Ränder füllen den Rest des Quadrats
    .tgs = Transparente Ränder füllen den Rest des Quadrats
square-crop = Durch Zuschneiden
    .hint = Die Mitte füllt das Quadrat, der Rest wird abgeschnitten: Die Vorschau dunkelt ihn ab
    .tgs = Die Mitte füllt das Quadrat, der Rest wird abgeschnitten
square-stretch = Durch Strecken
    .hint = Das Bild wird auf das Quadrat gestreckt
    .tgs = Die Pixel-Art wird auf das Quadrat gestreckt (ihre Pixel sind dann nicht mehr quadratisch)
square-exact = Ein fester ganzzahliger Faktor behält das ganze Bild: stattdessen auffüllen
scaling = Skalierung
    .hint = Wie aus den Pixeln der Quelle die des Ergebnisses werden
scaling-auto = Automatisch
    .hint = Scharfe Pixel für Pixel-Art, die auf das Doppelte oder mehr vergrößert wird, sonst weich
scaling-smooth = Weich
    .hint = Benachbarte Pixel gehen ineinander über
scaling-sharp = Scharfe Pixel
    .hint = Jedes Pixel wird zu einem Block, und Blöcke gehen nur an ihren Kanten ineinander über
scaling-integer = Ganzzahlig
    .hint = Jedes Pixel wird zu derselben ganzen Zahl von Ergebnispixeln, oder ganze Blöcke davon werden zu einem; transparente Ränder füllen den Rest
integer-scale = Ganzzahliger Faktor
    .hint = Zu wie vielen Ergebnispixeln jedes Quellpixel wird oder wie viele Quellpixel eines ergeben
    .only = Nur mit Skalierung „{ scaling-integer }“
scale-largest = Größter passender
    .hint = Das größte ganze Vielfache, bei dem der Zuschnitt ins Ergebnis passt
    .preset = Die Vorgabe legt einen Faktor fest
scale-fixed = Fest
    .hint = Dieser Faktor; der Zuschnitt kann höchstens so groß sein, wie damit ins Ergebnis passt
smaller = Kleiner
larger = Größer
scale-field = 3 für dreifache Größe oder 1/3 für ein Drittel eingeben
    .pick = „{ scale-fixed }“ wählen, um einen Faktor festzulegen
scale-unused = Ganzzahlige Faktoren gelten nur mit Skalierung „{ scaling-integer }“
scale-unsuited = Der Zuschnitt passt noch nicht zu diesem Faktor
scale-makes = { $scale } macht den Zuschnitt { $width } × { $height } px groß
scale-makes-padded = { $scale } macht den Zuschnitt { $width } × { $height } px groß; transparente Ränder füllen den Rest von { $box_width } × { $box_height }
scale-each-way = ×{ $across } horizontal und ×{ $down } vertikal
scale-shrinks = Der Zuschnitt ist größer als das Ergebnis und wird daher mit scharfen Pixeln verkleinert: Ein fester Faktor wie ×1/2 verkleinert ihn um ganze Blöcke
margins = Ränder
    .hint = Randzeilen und -spalten, die in allen Frames der Quelle transparent sind
auto-crop = Leere Ränder automatisch abschneiden
    .hint = Schneidet die Pixel-Art auf das kleinste Rechteck zu, das in irgendeinem Frame sichtbare Pixel enthält
    .after = Gilt nach dem Zuschnitt oben
art-pixel = Grafikpixelgröße
    .hint = Wie viele Quellpixel ein Grafikpixel einnimmt. Pixel außerhalb seines Rasters werden darauf verschoben
art-pixel-detected = Erkannt
    .hint = Aus der Pixel-Art ermittelt: ihre kleinsten einfarbigen Blöcke


## Settings: time

time = Zeit
used-part = Abschnitt
    .hint = Wo der verwendete Teil der Quelle beginnt und endet; die Zeitleiste unter der Vorschau legt ihn ebenfalls fest
part-from = Von
part-to = bis
part-first = Erster Frame
part-last = Letzter Frame
all-of-input = Gesamte Quelle
part-seconds = { $from } bis { $to } von { $length }
part-frame = Frame { $number } von { $count }
part-frames = Frames { $first } bis { $last } von { $count }
speed = Tempo
    .hint = Um welchen Faktor das Ergebnis schneller abläuft als die Quelle
from-preset = Wie Vorgabe
    .hint = Der Wert, den die Vorgabe festlegt
from-input = Wie Quelle
    .speed = So schnell, wie die Quelle abläuft
    .rate = Die Framerate der Quelle bei diesem Tempo, höchstens 60 fps
fits-3-s = Passt in 3 s
    .hint = So schnell, dass der Abschnitt in 3 Sekunden passt
fit-3-s = In 3 s einpassen
    .hint = Das Tempo so setzen, dass der Abschnitt in 3 Sekunden abläuft
plays = Läuft { $played }
plays-cut = Läuft { $kept } von { $used }: Der Rest wird abgeschnitten
plays-spoofed = Läuft ganze { $played }: Telegram wird eine kürzere Dauer gemeldet
frame-rate = Framerate
    .hint = Weniger Frames pro Sekunde lassen jedem mehr Bytes; mehr als bei der Quelle würden sie nur wiederholen
    .sequence = Wie schnell die Frames der Sequenz ablaufen
    .sheet = Wie schnell die Frames einer Sequenz oder eines Spritesheets ablaufen
default-rate = Standard
    .hint = { $fps } fps, da diese Frames nicht angeben, wie schnell sie ablaufen
over-3-s = Dauer fälschen
    .hint = Telegram lehnt Sticker ab, deren angegebene Dauer über 3 Sekunden liegt
spoof-auto = Wenn länger
    .hint = Sticker über 3 Sekunden bekommen eine kurze Dauer eingetragen: Telegram nimmt sie an und spielt sie ganz ab
spoof-always = Immer
    .hint = Auch kurze Sticker bekommen die kurze Dauer eingetragen
spoof-never = Nie: bei 3 s abschneiden
    .hint = Fälscht die Metadaten nicht und schneidet das Video bei 3 Sekunden ab
spoof-to = Gefälschte Dauer
    .hint = Die in den Metadaten eingetragene Dauer, die Telegram liest
    .honest = Mit „{ spoof-never }“ wird nichts gefälscht
default-duration = Standard
    .hint = Eine kurze Dauer, die Telegram annimmt
frame = Frame
frame-at = Frame bei
    .hint = Der Zeitpunkt des Frames, aus dem das Bild entsteht; die Zeitleiste unter der Vorschau wählt ihn ebenfalls


## Settings: size and quality of a WebM

size-and-quality = Größe und Qualität
fitting = Größenanpassung
    .hint = Was tgradish verändert, um knapp unter dem Dateigrößenlimit zu landen
    .lossless = Verlustfreie Kodierung lässt keine Bitrate oder Qualität zum Anpassen: Diese Wahl entfernt den Haken bei „{ lossless-encoding }“
fit-auto = Beste Optik
    .hint = Passt die Bitrate an und probiert niedrigere Frameraten, wo nichts passt oder weniger Frames mit je mehr Bits besser aussehen könnten; behält, was der Quelle am ähnlichsten sieht
fit-bitrate = Bitrate
    .hint = Passt die Bitrate bei der gewählten Framerate an
fit-crf = Qualität
    .hint = Passt den Wert für konstante Qualität (CRF) an
fit-fps = Framerate
    .hint = Passt die Framerate bei konstanter Qualität an
fit-length = Länge
    .hint = Passt die Länge bei konstanter Qualität an und schneidet das Ende ab
fit-off = Aus
    .hint = Kodiert einmal mit den Werten unten, auch wenn das Ergebnis zu groß wird
encoder = Encoder-Aufwand
    .hint = Schnelleres Kodieren sieht bei gleicher Größe schlechter aus
encoder-fast = Schnell
    .hint = Dauert Sekunden, sieht bei gleicher Größe aber sichtbar schlechter aus
encoder-balanced = Ausgewogen
encoder-best = Gründlich
    .hint = Langsam: dauert bei langen Videos Minuten
fitting-off = „{ fit-off }“ kodiert einmal
attempts = Kodiert bis zu
    .hint = Wie oft die Größenanpassung höchstens kodieren darf
attempts-default = Standard
    .hint = Genug, um bei den meisten Quellen bis auf wenige Prozent an das Dateigrößenlimit heranzukommen
attempts-times = Mal
search-within = Suchbereich
    .hint = Der Bereich, den die Größenanpassung durchsucht, in der Einheit des angepassten Werts
    .low = Untergrenze
    .high = Obergrenze
search-auto = Automatisch
    .hint = So weit, wie es zur Art der Anpassung und zur Quelle passt
range-to = bis
crf = Qualität (CRF)
    .hint = Konstante Qualität von 0, der besten, bis 63
    .lossless = Verlustfreie Kodierung braucht keinen Qualitätswert
    .fitted = Die Größenanpassung ermittelt die Qualität
    .bitrate = Die Größenanpassung ermittelt eine Bitrate, keine Qualität
crf-default = Standard
    .hint = Gute Qualität bei Stickergrößen
lossless = Verlustfrei
lossless-encoding = Verlustfreie Kodierung
    .hint = Nur bei winzigen oder unbewegten Bildern klein
    .switches = Nur bei winzigen oder unbewegten Bildern klein. Verlustfreie Kodierung lässt keine Bitrate oder Qualität zum Anpassen: Das Ankreuzen stellt die Größenanpassung auf „{ fit-fps }“ um
bitrate = Bitrate
    .hint = Wird verwendet, sofern nicht nur eine Qualität festgelegt ist
    .fitted = Die Größenanpassung ermittelt die Bitrate
    .constant = Mit konstanter Qualität kodiert, nicht mit einer Bitrate
bitrate-auto = Aus dem Dateigrößenlimit
    .hint = So viel, wie bei dieser Länge hineinpasst


## Settings: size and quality of a TGS animation

tgs-encoder = Encoder-Aufwand
    .hint = Wie gründlich nach einer kleinen Kodierung gesucht wird
tgs-fast = Schnell
    .hint = Dauert unter einer Sekunde, einige Prozent größer
tgs-balanced = Ausgewogen
tgs-best = Gründlich
    .hint = Dauert einige Sekunden, kleinste Datei
too-large = Wenn es nicht passt
    .hint = Was passiert, wenn die Pixel-Art unverändert nicht in 64 KiB passt
never-change = Pixel-Art nie verändern, stattdessen melden
    .set = Die Pixel-Art wird nie verändert, wie oben eingestellt
give-up = Zuerst opfern
    .hint = Wenn sich die Pixel-Art ändern muss, um zu passen: ihre Bewegung oder ihre Details
give-up-either = Beliebig
    .hint = Was die Pixel-Art für die gesparten Bytes am wenigsten verändert
give-up-motion = Bewegung
    .hint = Frames werden zuerst zusammengeführt und weggelassen: volle Details, ruckligere Bewegung
give-up-detail = Details
    .hint = Das Bild wird zuerst gröber: flüssige Bewegung, weniger Details
ways-to-fit = Mittel zum Einpassen
    .hint = Was die Größenanpassung an der Pixel-Art ändern darf, um sie in 64 KiB unterzubringen, das Unauffälligste zuerst
reduce-snap = Am Pixelraster ausrichten
    .hint = Verschiebt verirrte Pixel auf das Raster der Grafikpixel
reduce-colours = Ähnliche Farben zusammenführen
    .hint = Kaum unterschiedliche Farben werden zu einer
reduce-specks = Störpixel entfernen
    .hint = Einzelne Pixel, die sich von allen umliegenden unterscheiden
reduce-frames = Ähnliche Frames zusammenführen
    .hint = Kaum unterschiedliche Frames zeigen dasselbe Bild
reduce-drop = Frames weglassen
    .hint = Zeigt nur jeden zweiten Frame oder weniger
reduce-resolution = Auflösung verringern
    .hint = Weniger, dafür größere Grafikpixel
reading-art = Lesen der Pixel-Art
sprite-sheet = Spritesheet
    .hint = Frames in einem Raster gleich großer Zellen, Zeile für Zeile
read-as-sheet = Als Spritesheet lesen
sheet-refused = Diese Datei hat bereits { $frames } Frames: Ein Spritesheet ist ein einzelnes Bild
    .converted = Unverändert gelesen, nicht als Spritesheet: Die Datei hat bereits { $frames } Frames
sheet-cells = Zellen
    .columns = Spalten
    .rows = Zeilen
sheet-frames = Frames
sheet-all = Alle
    .hint = Alle Zellen außer transparenten am Ende


## Settings: an Aseprite file's tags and layers

aseprite-file = Aseprite-Datei
unreadable-choices = Mit dem gewählten Tag und den gewählten Ebenen nicht lesbar
read-all = Alles lesen
tag = Tag
    .hint = Wählt die Frames eines Tags: von seinem Beginn bis zu seinem Ende oder in eigener Reihenfolge, wenn er rückwärts oder hin und zurück läuft
all-frames = Alle Frames
    .hint = Jeder Frame der Datei
tag-frame = Frame { $number }
tag-frames = Frames { $from } bis { $to }
tag-reverse = { $frames }, rückwärts
tag-ping-pong = { $frames }, hin und zurück
tag-pong-ping = { $frames }, zurück und hin
layers = Ebenen
    .hint = Welche Ebenen sichtbar sind, die oberste zuerst; eine ausgeblendete Gruppe blendet ihre Ebenen aus
    .group-hidden = Die Gruppe ist ausgeblendet
    .file = Zurück zu den Ebenen, die die Datei zeigt


## Settings: encoder options, folded away under the rest

encoder-options = Encoder-Optionen
vp9-options = Encoder-Optionen nach Namen, etwa tune-content = screen für flache Grafiken. ffmpeg -h encoder=libvpx-vp9 listet sie auf
    .name = Zum Beispiel: tune-content
    .value = Zum Beispiel: screen
add-option = Option hinzufügen
examples = Beispiele:
example-tune-content = Flache Grafiken und Bildschirmaufnahmen: schärfere Kanten
example-aq-mode = Setzt Bits dort ein, wo das Bild komplex ist
example-sharpness = Erhält mehr feine Details, auf Kosten etwas Glätte
example-arnr-strength = Weniger Rauschminderung über Frames hinweg
example-g = Mindestens alle 60 Frames ein Keyframe
example-qmax = Nirgends schlechter als Qualität 50
ffmpeg-arguments = ffmpeg-Argumente
    .hint = Rohe Argumente vor der Ausgabe, aufgeteilt wie in einer Shell
    .placeholder = Zum Beispiel: -qmin 4
    .unfinished = Ein Anführungszeichen oder Backslash ist offen: Bis es geschlossen ist, gelten die Argumente wie zuletzt gelesen
    .system = Nur mit dem ffmpeg des Systems, das in „{ settings }“ gewählt werden kann


## What keeps a file from being converted as it is, and the ways out

blocked = Noch nicht konvertierbar: { $reason }
    .heading = Noch nicht konvertierbar
blocked-crop = Der Zuschnitt liegt außerhalb des Bildes
blocked-sheet = Die Zellen des Spritesheets teilen das Bild nicht gleichmäßig auf
blocked-tag = Die Datei hat keinen Tag „{ $tag }“
blocked-layers = { $count ->
    [1] Die Datei hat keine Ebene { $layers }
   *[other] Die Datei hat keine Ebenen { $layers }
}
blocked-ffmpeg = ffmpeg wurde nicht gefunden
crop-outside = Der Zuschnitt, { $width } × { $height } ab { $left }, { $top }, liegt außerhalb dieses Bildes von { $across } × { $down }
use-whole = Ganzes Bild verwenden
    .hint = Nichts abschneiden und das ganze Bild behalten
sheet-uneven = Das Bild, { $width } × { $height }, lässt sich nicht in { $columns } × { $rows } gleich große Zellen teilen
sheet-nearest = Nächste passende Aufteilung: { $columns } × { $rows }
    .hint = Die Aufteilung, die dieser am nächsten kommt und deren Zellen das Bild gleichmäßig aufteilen
tag-missing = Diese Datei hat keinen Tag „{ $tag }“
layers-missing = { $count ->
    [1] Diese Datei hat keine Ebene { $layers }
   *[other] Diese Datei hat keine Ebenen { $layers }
}
layers-as-file = Ebenen wie in der Datei
    .hint = Ebenen so ein- und ausblenden, wie es die Datei tut
needs-ffmpeg = Dafür wird ffmpeg benötigt, das nicht gefunden wurde
    .hint = ffmpeg aus den Paketen des Systems oder von ffmpeg.org installieren oder seinen Speicherort wählen. Sobald es gefunden ist, werden die Dateien erneut gelesen
choose-ffmpeg = ffmpeg auswählen…
    .hint = Das ffmpeg-Programm wählen oder einen Ordner mit ffmpeg und ffprobe
row-attention = Eingriff nötig
    .hint = Etwas verhindert die Konvertierung: siehe Einstellungen der Datei
row-needs-ffmpeg = braucht ffmpeg
    .hint = ffmpeg liest die Datei oder erstellt ihr Ergebnis und wurde nicht gefunden
need-attention = { $count ->
    [one] 1 erfordert Eingriff
   *[other] { $count } erfordern Eingriff
}


## What converting will do, beside the preview, step by step

plan-video = Das Video wird:
plan-picture = Das Bild wird:
plan-animation = Die Animation wird:
plan-art = Die Pixel-Art wird:
plan-video-frame = Ein Frame des Videos wird:
plan-animation-frame = Ein Frame der Animation wird:
plan-art-frame = Ein Frame der Pixel-Art wird:
plan-tag = Als Tag „{ $tag }“ abgespielt
plan-tag-reverse = Als Tag „{ $tag }“ rückwärts abgespielt
plan-tag-ping-pong = Als Tag „{ $tag }“ hin und zurück abgespielt
plan-tag-pong-ping = Als Tag „{ $tag }“ zurück und hin abgespielt
plan-cropped = Auf { $width } × { $height } zugeschnitten
plan-frame = Frame bei { $time } entnommen
plan-scaled = Auf { $width } × { $height } skaliert
plan-stretched-across = Horizontal um ×{ $factor } gestreckt
plan-stretched-down = Vertikal um ×{ $factor } gestreckt
plan-middle-cut = Mitte auf { $width } × { $height } zugeschnitten
plan-padded = Auf { $width } × { $height } aufgefüllt
plan-trimmed = Auf die sichtbaren Pixel zugeschnitten
plan-grid = Pixel an einem Raster von { $scale } × { $scale } ausgerichtet
plan-square-crop = Quadratisch zugeschnitten
plan-square-stretch = Quadratisch gestreckt
plan-square-pad = Quadratisch aufgefüllt
plan-cut = Auf { $from }–{ $to } von { $length } gekürzt
plan-faster = Um ×{ $factor } beschleunigt
plan-slower = Um ×{ $factor } verlangsamt
plan-cut-3-s = Bei 3 s von { $played } abgeschnitten
plan-spoofed = Dauer gefälscht, damit alle { $played } abgespielt werden
plan-spoofed-anyway = Dauer gefälscht, obwohl { $played } kurz genug ist
plan-saved = Als { $name } gespeichert


## Settings: the same conversion as commands, folded away under the rest

commands = Befehle
copy = Kopieren
commands-posix = Dieselbe Konvertierung auf der Kommandozeile, für eine POSIX-Shell:
commands-powershell = Dieselbe Konvertierung auf der Kommandozeile, für PowerShell:
pasted-temporary = Ein eingefügtes Bild liegt in einem temporären Ordner, der beim Beenden von tgradish gelöscht wird
ffmpeg-commands = Die ffmpeg-Befehle der ersten Kodierung; die Größenanpassung ändert zwischen den Kodierungen die Rate und eventuell die Framerate:
    .builtin = Was das integrierte ffmpeg bei der ersten Kodierung tut, als Befehle für das ffmpeg des Systems; die Größenanpassung ändert zwischen den Kodierungen die Rate und eventuell die Framerate:
no-ffmpeg-commands = Keine ffmpeg-Befehle: { $why }
    .unread = Die Quelle ist noch nicht gelesen
    .art = ffmpeg liest ein PNG, das jede Konvertierung aus der Pixel-Art zeichnet


## The Settings dialog

save = Speichern
saved-to = Gespeichert unter { $path }
    .kept = Gespeichert unter { $path }; die nicht lesbare Datei liegt jetzt als { $kept } vor
not-saved = Speichern fehlgeschlagen: { $error }
no-home = Es gibt keinen Benutzerordner für die Einstellungen
settings-note = Änderungen gelten sofort und werden direkt gespeichert
reset-settings = Auf Standard zurücksetzen
    .hint = Alle Einstellungen hier auf den ursprünglichen Stand von tgradish zurücksetzen
ffmpeg-given = Diese Sitzung nutzt das beim Start von tgradish angegebene ffmpeg; die Einstellungen hier gelten bei einem Start ohne diese Angabe
ffmpeg-use = Verwenden
    .hint = Videos werden mit ffmpeg gelesen und WebM-Sticker damit erstellt
ffmpeg-auto = Automatisch
    .hint = Das integrierte ffmpeg, falls dieser Build eines hat, sonst das des Systems
ffmpeg-builtin = Integriert
    .hint = Das in tgradish enthaltene ffmpeg
    .missing = Dieser Build von tgradish enthält kein ffmpeg
ffmpeg-system = System
    .hint = ffmpeg und ffprobe im PATH; erlaubt rohe ffmpeg-Argumente
ffmpeg-program = Programm
    .hint = Eine ffmpeg-Programmdatei oder ein Ordner mit ffmpeg und ffprobe; hat Vorrang vor der Wahl oben
    .missing = Dort ist nichts, daher bleibt das verwendete Programm unverändert
ffmpeg-found = { $path }, im PATH gefunden
for-example = Zum Beispiel: { $example }
choose = Auswählen…
check = Prüfen
    .hint = Prüfen, ob dieses ffmpeg Sticker erstellen kann
checking-ffmpeg = Wird geprüft…
ffmpeg-ready = ffmpeg { $version } mit VP9: bereit
ffmpeg-no-vp9 = ffmpeg { $version } kann VP9 nicht mit libvpx kodieren
converting-settings = Konvertierung
threads = Threads
    .hint = Wie viele die Konvertierung höchstens gleichzeitig nutzt; weniger lassen dem Rest des Computers mehr übrig und dauern länger
threads-auto = Automatisch
    .hint = So viele, wie jeder Teil der Konvertierung nutzen kann
results = Ergebnisse
save-results = Speichern
next-to-input = Neben jeder Quelldatei
in-a-folder = In einem Ordner
    .unchosen = Bis ein Ordner gewählt ist, werden Ergebnisse neben jeder Quelldatei gespeichert
    .relative = Den vollständigen Pfad ab der obersten Ebene eingeben, damit der verwendete Ordner wechseln kann
    .file = Das ist eine Datei, kein Ordner, daher bleibt der verwendete Ordner unverändert
existing-files = Vorhandene Dateien
    .hint = Von tgradish erstellte Ergebnisse werden beim erneuten Erstellen immer ersetzt
replace-in-the-way = Jede im Weg liegende Datei ersetzen
interface = Oberfläche
language = Sprache
    .hint = Die Sprache dieser Oberfläche; die Kommandozeile bleibt auf Englisch
    .variable = Von der Umgebungsvariable { $variable } festgelegt
as-the-system = Wie das System
as-the-system-language = Wie das System: { $language }
theme = Design
    .hint = Die Schaltfläche neben ⚙ wechselt ebenfalls zwischen hell und dunkel
theme-light = Hell
theme-dark = Dunkel
accent = Akzentfarbe
    .hint = Kennzeichnet Ausgewähltes, Elemente unter dem Mauszeiger und den Fokus
scale = Skalierung
    .hint = Wie groß Text und Bedienelemente sind
    .keys = { $larger } und { $smaller } ändern sie ebenfalls; { $reset } setzt sie auf 100% zurück
scale-smaller = Kleiner
scale-larger = Größer
scale-reset = Zurück auf 100%
scale-percent = { $percent }%
accent-system = { as-the-system }
    .hint = Die Akzentfarbe des Desktops oder das Blau von Breeze, wenn er keine hat
accent-pink = Rosa
accent-red = Rot
accent-orange = Orange
accent-yellow = Gelb
accent-green = Grün
accent-teal = Türkis
accent-blue = Blau
accent-purple = Lila
accent-violet = Violett
accent-grey = Grau
accent-own = Eigene Farbe
    .chosen = Eigene Farbe, { $colour }
accent-shade = Schattierung
    .value = { $saturation }% Sättigung, { $value }% Helligkeit
accent-hue = Farbton
accent-hex = Hex
    .wrong = Als # und sechs Hexadezimalziffern schreiben, etwa #3daee9
files = Dateien
reopen-files = Dateien vom letzten Mal mit ihren Einstellungen wieder öffnen
motion = Animationen
    .hint = Wenn das System weniger Bewegung wünscht, bewegt sich in keinem Fall etwas
motion-on = Weich scrollen und Änderungen animieren
new-files = Neue Dateien
preset-for-webm = Vorgabe für WebM
preset-for-tgs = Vorgabe für TGS


## The result, beside the preview

last-conversion = Letzte Konvertierung
path-in-the-way = { $path } ist im Weg
stopped-early = Vor dem Abschluss gestoppt
result = Ergebnis
    .stale = Ergebnis, veraltet
result-copy = Kopieren ({ $keys }), um es in einen Telegram-Chat einzufügen
result-webm = { $width } × { $height } px, { $fps } fps, { $length }
    .spoofed = { $width } × { $height } px, { $fps } fps, { $length }, Dauer gefälscht
changed-to-fit = Zum Einpassen geändert: wie, steht unten
replaced-earlier = Ein früheres Ergebnis von tgradish wurde ersetzt
converting = Konvertierung
stage-reading = Lesen der Pixel-Art
stage-drawing = Umwandeln in Vektoren
stage-fitting-steps = { $count ->
    [one] Einpassen in 64 KiB: { $count } Schritt, etwa { $size }
   *[other] Einpassen in 64 KiB: { $count } Schritte, etwa { $size }
}
stage-fitting-from = Einpassen in 64 KiB ab { $size }
stage-compressing = Komprimieren
    .hopeless = Schnelles Komprimieren: Es kann nicht passen
    .missed = Komprimiert → { $size }, zu groß
    .missed-far = Komprimieren: immer noch viel zu groß
retry = Versuch { $number }: Ziel { $aim }
retry-done = Versuch { $number }: Ziel { $aim } → { $size }, zu groß
    .far = Versuch { $number }: Ziel { $aim } → immer noch viel zu groß
starting = Wird gestartet
encode = Kodierung { $number }: { $fps } fps, { $rate }
encode-of = Kodierung { $number } von höchstens { $most }: { $fps } fps, { $rate }
encode-done = Kodierung { $number }: { $fps } fps, { $rate } → { $size }, passt
    .too-large = Kodierung { $number }: { $fps } fps, { $rate } → { $size }, zu groß
rate-quality = Qualität { $crf }
rate-lossless = verlustfrei
encodes-up-to = { $count ->
    [one] Bis zu { $count } Kodierung, bis eine knapp unter dem Dateigrößenlimit landet
   *[other] Bis zu { $count } Kodierungen, bis eine knapp unter dem Dateigrößenlimit landet
}
art-size = { $width } × { $height } Grafikpixel
    .scaled = { $width } × { $height } Grafikpixel, je { $scale } × { $scale } Quellpixel
art-summary = { $size }; { $colours }, { $frames }
art-tempo = Tempo ×{ $tempo } gegenüber der Quelle
art-off-grid = Das meiste sieht nach { $scale }×-Pixel-Art aus, mit einigen Pixeln außerhalb des Rasters; eine Grafikpixelgröße von { $scale } richtet sie aus
art-shapes = { $layers } aus { $rectangles }, { $size } entpackt
art-before-fitting = Unverändert etwa { $size }; zum Einpassen geändert:
art-step = { $change }, danach etwa { $size }
step-snapped = Pixel an einem Raster von { $scale } ausgerichtet
step-colours = Farben mit weniger als { $distance }% Abstand zusammengeführt
step-frames = Frames mit weniger als { $changed }% Änderung zusammengeführt
step-dropped = { $share }% der Frames weggelassen
step-specks = Einzelne Störpixel entfernt
step-resolution = Auflösung auf { $factor }% verringert
column-number = Jede Kodierung der Reihe nach; ✔ die behaltene
column-fps = Frames pro Sekunde
column-rate = Bitrate oder die Qualität, mit der kodiert wurde
column-size = Größe
    .hint = Anteil am Dateigrößenlimit, grün, wenn es passt
column-alike = Ähnlich
    .hint = Wie sehr es der Quelle ähnelt, gemessen mit SSIM (strukturelle Ähnlichkeit)
rate-quality-short = q { $crf }
decided-in-band = Gestoppt: Kodierung { $number } nutzt { $share } des Limits, mindestens die angestrebten { $least }
decided-alike-enough = Gestoppt: Kodierung { $number } ähnelt der Quelle bereits zu { $alike }, mehr Bits würden sie nur größer machen
decided-alike-unchanged = Gestoppt: Kodierung { $number } ähnelt der Quelle zu { $alike }, kaum mehr als Kodierung { $than } mit weniger Bits
decided-little-to-gain = Gestoppt: Kodierung { $number } nutzt { $share } des Limits, eine weitere käme nur auf etwa { $predicted }
decided-too-large-at-least = Gestoppt: Selbst der kleinste Wert des Bereichs nutzt in Kodierung { $number } { $share } des Limits
decided-nothing-left = Gestoppt: Keine Einstellung ist mehr einen Versuch wert
decided-out-of-attempts = { $count ->
    [one] Gestoppt: Die { $count } erlaubte Kodierung ist erfolgt
   *[other] Gestoppt: Alle { $count } erlaubten Kodierungen sind erfolgt
}
decided-lowers-to-fit = Nächster Versuch mit { $fps } fps: Bei { $from } fps passte nichts
decided-lowers-for-looks = Nächster Versuch mit { $fps } fps: Kodierung { $number } mit { $from } fps ähnelt der Quelle nur zu { $alike }
decided-settled = Vorzeitig gestoppt: Kodierung { $number } behalten
decided-lower-no-better = Gestoppt: Mit { $fps } fps ähnelt es der Quelle zu { $alike }, nicht mehr als die { $kept_alike } von Kodierung { $kept } mit { $kept_fps } fps
issue-not-webm = Der Container ist { $container }, nicht WebM
issue-no-video = Es hat keine Videospur
issue-not-vp9 = Der Codec ist { $codec }, nicht VP9
issue-audio = Es hat eine Audiospur
issue-size = Die Größe ist { $width } × { $height } px, Telegram verlangt aber { $side } × { $side } px
    .sticker = Die Größe ist { $width } × { $height } px, Telegram verlangt aber eine Seite mit { $side } px und die andere höchstens so lang
issue-too-big = Es ist { $size } groß und überschreitet das Limit von { $limit }
issue-too-long = Laut Metadaten dauert es { $seconds } und überschreitet das Limit von { $limit }
issue-too-many-fps = Die Framerate ist { $fps } fps und überschreitet das Limit von { $limit } fps
issue-animated = Es ist animiert, statische Sticker sind aber ein einzelnes Bild


## Inspecting a sticker

inspect-another = Weiteren prüfen…
inspect-webm-sticker = Ein Video-Sticker
inspect-webm-emoji = Ein Video-Emoji
inspect-image-sticker = Ein statischer Sticker
inspect-image-emoji = Ein statisches Emoji
inspect-tgs = Eine Vektoranimation, { $width } × { $height } mit { $fps } fps
inspect-unknown = Eine Stickerart, die diese Version nicht anzeigen kann
inspect-size = Größe
    .hint = Telegram lehnt größere Dateien ab
    .tgs = Telegram lehnt größere Sticker ab
inspect-duration = Dauer
    .hint = Telegram liest die Dauer aus den Metadaten; Fälschen macht sie kurz
    .spoofed = { $seconds } s in den Metadaten, gefälscht
    .no-header = { $seconds }, keine in den Metadaten
inspect-plays = Laufzeit
    .hint = Wie lang das Video wirklich ist
inspect-frame-rate = Framerate
    .value = { $fps } fps von { $most }, { $frames }
inspect-picture = Bild
    .none = keins
inspect-video = { $width } × { $height } px, { $codec }
    .alpha = { $width } × { $height } px, { $codec }, mit Transparenz
inspect-webp = { $width } × { $height } px, WebP, verlustfrei
    .alpha = { $width } × { $height } px, WebP, verlustfrei, mit Transparenz
    .lossy = { $width } × { $height } px, WebP, verlustbehaftet
    .lossy-alpha = { $width } × { $height } px, WebP, verlustbehaftet, mit Transparenz
inspect-audio = Audio
inspect-title = Titel
inspect-made-with = Erstellt mit
    .hint = Die Programme, die die Datei geschrieben haben, laut der Datei selbst
    .image = Das Programm, das die Metadaten nennen
    .unknown = unbekannt
inspect-note = Notiz
    .hint = Text in den Füllbytes der Datei, den Player überspringen
inspect-name = Name
    .hint = Der eigene Name der Animation, den Telegram nicht anzeigt
inspect-length = Länge
    .hint = Animierte Sticker dauern höchstens 3 Sekunden
    .value = { $seconds } von 3.00 s, { $frames }
inspect-cost = Zeichenaufwand
    .hint = Formen plus 9 für jede Ebene, so wie Telegram zählt
inspect-layers = Ebenen
inspect-shapes = Max. Formen pro Ebene
inspect-refuses-more = Telegram lehnt mehr ab
inspect-points = Max. Punkte pro Füllung
    .hint = Pfadpunkte, die eine Füllung malt; Telegram lehnt mehr ab
inspect-unpacked = Entpackt
    .hint = Telegram Desktop spielt größere Animationen nicht ab
inspect-features = Funktionen
    .hint = Lottie-Funktionen über Formen und Transformationen hinaus


## What went wrong, as converting or reading a file reports it

crashed = tgradish ist abgestürzt: { $details }
ffmpeg-not-found = ffmpeg wurde nicht gefunden: { $details }
ffmpeg-not-on-path = ffmpeg und ffprobe sind nicht im PATH: sie installieren oder in „{ settings }“ ein anderes ffmpeg wählen
ffmpeg-args-need-program = ffmpeg-Argumente erfordern das ffmpeg des Systems, das in „{ settings }“ gewählt werden kann; Encoder-Optionen funktionieren mit beiden
program-failed = { $program } ist fehlgeschlagen ({ $status }):
unreadable-file = { $name } konnte nicht gelesen werden: { $details }
no-video = { $name } enthält kein Video
no-such-file = { $name } ist nicht mehr vorhanden
not-webm = { $name } ist keine WebM-Datei
not-sticker = { $name } ist kein Sticker, also keine WebM-, WebP- oder .tgs-Datei
no-frame-at = { $name } hat keinen Frame bei { $seconds }
nothing-fits = Nichts passt in das Limit von { $limit }: Die kleinste Kodierung war { $smallest }. Ein kürzerer Abschnitt, eine niedrigere Framerate oder ein größerer Suchbereich können helfen
clipboard-failed = Einfügen aus der Zwischenablage fehlgeschlagen: { $details }
output-is-input = Das Ergebnis würde seine eigene Quelle { $name } überschreiben
nothing-to-convert = Es gibt keine Bilder zum Konvertieren


## Saving and managing presets

save-preset-title = { -save-settings }
save-preset-note = Die Einstellungen dieser Datei als Ausgangspunkt für jede Datei, außer denen, die nur zu ihr passen: Sticker oder Emoji, Zuschnitt, Abschnitt, Tag, Ebenen und Spritesheet
preset-name = Name
    .placeholder = meine Sticker
preset-description = Beschreibung
    .placeholder = optional
    .none = keine Beschreibung
preset-exists = Eine Vorgabe mit diesem Namen gibt es schon: Speichern ersetzt sie
replace = Ersetzen
name-it-first = Erst einen Namen eingeben
cancel = Abbrechen
preset-name-length = Der Name einer Vorgabe hat 1 bis { $most } Zeichen und beginnt nicht mit einem Leerzeichen
preset-name-characters = Der Name einer Vorgabe enthält nur Buchstaben, Ziffern, Leerzeichen, - und _
preset-name-builtin = „{ $name }“ ist der Name einer integrierten Vorgabe
preset-name-taken = Eine andere Vorgabe heißt bereits „{ $name }“
preset-extended = „{ $other }“ baut darauf auf: zuerst diese Vorgabe ändern
your-presets = Eigene Vorgaben
your-presets-note = Änderungen werden sofort gespeichert. „{ preset-fast }“, „{ preset-balanced }“ und „{ preset-best }“ sind integriert und bleiben unverändert
no-presets = Noch keine: „{ -save-settings }“ in der Leiste unter den Einstellungen
preset-broken = Die Datei ist nicht lesbar
delete-it = Löschen
deleted = „{ $name }“ gelöscht
undo = Rückgängig
    .hint = Zurückholen
not-put-back = Die Vorgabe „{ $name }“ konnte nicht zurückgeholt werden


## About and licences

about-title = Info und Lizenzen
about-summary = Telegram-Sticker und -Emojis aus Videos, Bildern oder Pixel-Art. MIT-Lizenz;
source-code = Quellcode
send-feedback = Feedback auf Telegram senden
about-notices = tgradish baut auch auf der Arbeit anderer auf. Deren Lizenzen verlangen, dass diese Hinweise jeder Kopie beiliegen, daher enthält das Programm sie:
notice-licence = { $name }: Lizenz
notice-named-licence = { $name }: { $licence }
notice-licence-notes = { $name }: Lizenzhinweise
notice-patent-grant = { $name }: Patentgewährung
notice-sources = { $name }: Enthaltene Teile und ihr Quellcode
notice-font = { $subsets ->
    [1] Schriftart: { $name }, Teilmenge
   *[other] Schriftart: { $name }, Teilmengen
}
notice-crates = Rust-Crates
notice-crates-licence = { $licence }: { $crates }
    .more = { $more ->
        [one] { $licence }: { $crates } und { $more } weiteres
       *[other] { $licence }: { $crates } und { $more } weitere
    }
ffmpeg-sources = Der genaue Quellcode des integrierten ffmpeg, zu dieser Version


## The dialog of keyboard shortcuts, opened from the menu or by ? or F1

key-ctrl = Strg
key-cmd = Cmd
key-alt = Alt
key-shift = Umschalt
key-enter = Eingabe
key-space = Leertaste
key-delete = Entf
key-backspace = Rücktaste
key-home = Pos1
key-end = Ende
key-escape = Esc
key-left = ←
key-right = →
key-up = ↑
key-down = ↓
shortcuts-files = Dateien
shortcuts-playing = Vorschau und Zeitleiste
shortcuts-general = Allgemein
shortcuts-mouse = Maus, in Vorschau und Zeitleiste
shortcut-add-files = Dateien hinzufügen
shortcut-paste = Dateien oder Bild aus der Zwischenablage hinzufügen
shortcut-select = Datei darüber oder darunter wählen
shortcut-convert = Gewählte Datei konvertieren
shortcut-convert-all = Alle konvertieren: jede Datei ohne aktuelles Ergebnis
shortcut-keep-best = Bisher beste Kodierung behalten, während ein WebM eingepasst wird
shortcut-copy-result = Ergebnis der gewählten Datei kopieren, um es in Telegram einzufügen
shortcut-remove = Gewählte Datei entfernen oder die, deren Zeile den Fokus hat
shortcut-undo = Änderung an der gewählten Datei rückgängig machen oder entfernte Dateien zurückholen
shortcut-redo = Rückgängig gemachte Änderung wiederherstellen
shortcut-play = Abspielen oder pausieren
shortcut-step = Einen Frame zurück oder vor; mit Umschalt zehn
shortcut-ends = Erster oder letzter verwendeter Frame
shortcut-cut-here = Abschnitt beim angezeigten Frame beginnen oder enden lassen
shortcut-move-start = Beginn des Abschnitts verschieben; mit Umschalt um zehn Frames
shortcut-move-end = Ende des Abschnitts verschieben; mit Umschalt um zehn Frames
shortcut-settings = Einstellungen öffnen
shortcut-inspect = Sticker prüfen
shortcut-scale-up = Alles vergrößern
shortcut-scale-down = Alles verkleinern
shortcut-scale-reset = Zurück auf 100%
shortcut-shortcuts = Diese Liste
shortcut-close = Dialog oder Menü schließen
with-keys = { $hint } ({ $keys })
mouse-drag = Ziehen
    .does = Zuschneiden oder den Zuschnitt bzw. seine Kanten verschieben
mouse-shape = Umschalt+Ecke ziehen
    .does = Form des Zuschnitts beibehalten, sofern kein Seitenverhältnis festgelegt ist
mouse-centred = Alt+Kante ziehen
    .does = Größe des Zuschnitts um seine Mitte ändern
mouse-free = Strg+Ziehen
    .does = Zuschneiden ohne Einrasten an Rändern und Mitte des Bildes
mouse-cancel = Esc beim Ziehen
    .does = Zuschnitt auf den vorherigen Stand zurücksetzen
mouse-crop-keys = Zuschnitt anklicken, dann Pfeiltasten
    .does = Um ein Pixel verschieben, mit Umschalt um zehn; Strg+Pfeiltasten ändern die Größe
mouse-double-click = Doppelklick
    .does = Zurück zum ganzen Bild
mouse-wheel = Mausrad
    .does = Hinein- oder herauszoomen
mouse-pan = Mit rechter oder mittlerer Taste ziehen
    .does = Verschieben, wenn hineingezoomt
mouse-pan-across = Umschalt+Mausrad
    .does = Seitwärts verschieben, wenn hineingezoomt
mouse-slide = Umschalt+Klammer ziehen
    .does = Abschnitt auf der Zeitleiste verschieben, seine Länge bleibt
