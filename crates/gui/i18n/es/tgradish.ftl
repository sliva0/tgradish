### tgradish's words in Spanish. What each message means, and where it is
### shown, is said in the comments of en-GB/tgradish.ftl.

language-name = Español


## Shared by many places

close = Cerrar
back-to-preset = Volver al valor del preajuste
custom = Manual
    .hint = Elegir «{ custom }» para definirlo
webm-sticker = Sticker WebM
webm-emoji = Emoji WebM
tgs-sticker = Sticker TGS
tgs-emoji = Emoji TGS
image-sticker = Sticker estático
image-emoji = Emoji estático
count-colours = { $count ->
    [one] { $count } color
    [many] { $count } colores
   *[other] { $count } colores
}
count-frames = { $count ->
    [one] { $count } fotograma
    [many] { $count } fotogramas
   *[other] { $count } fotogramas
}
count-layers = { $count ->
    [one] { $count } capa
    [many] { $count } capas
   *[other] { $count } capas
}
count-rectangles = { $count ->
    [one] { $count } rectángulo
    [many] { $count } rectángulos
   *[other] { $count } rectángulos
}
count-tracks = { $count ->
    [one] { $count } pista
    [many] { $count } pistas
   *[other] { $count } pistas
}
of-limit = { $used } de { $limit }
telegram-accepts = Telegram debería aceptarlo


## Messages along the bottom

dismiss = Descartar
    .all = Descartar todos
said-times = ×{ $count }
details = Detalles
settings-unreadable = No se puede leer el archivo de ajustes, así que se usan los valores predeterminados
setting-unsaved = No se pudo guardar el cambio en el archivo de ajustes
    .unreadable = El cambio no se guardó: no se puede leer el archivo de ajustes
settings-file-note = Puede que se haya editado a mano o con otra versión de tgradish. Los cambios no se podrán guardar hasta que se corrija o se reemplace; no se pierde nada de lo que contiene.
fresh-settings = Empezar con ajustes nuevos
    .hint = Escribir los ajustes actuales en un archivo de ajustes nuevo; el anterior se conserva al lado
    .saved = Se guardaron los ajustes nuevos
    .kept = El archivo anterior se conserva al lado, como { $name }
    .failed = No se pudieron escribir los ajustes nuevos
read-again = Reintentar
    .hint = Volver a leerlo, una vez corregido
    .failed = Sigue sin poder leerse
settings-read = El archivo de ajustes ya se puede leer y sus ajustes están en uso
copy-path = Copiar ruta
    .hint = Copiar la ubicación del archivo, para abrirlo en un editor
    .done = Copiada
files-removed = { $count ->
    [one] Se quitó { $count } archivo
    [many] Se quitaron { $count } archivos
   *[other] Se quitaron { $count } archivos
}
undo-removing = Deshacer
    .hint = Devolverlos a su lugar, con sus ajustes y resultados (Ctrl+Z)
settings-reset = Se restablecieron los ajustes predeterminados
undo-reset = Deshacer
    .hint = Recuperar los ajustes que había antes
folder-of = Mostrar en carpeta
    .hint = Abrir la carpeta que lo contiene
presets-unreadable = No se pudieron leer los preajustes propios, así que solo se ofrecen los integrados
files-gone = { $count ->
    [one] { $count } archivo de la última vez ya no está
    [many] { $count } archivos de la última vez ya no están
   *[other] { $count } archivos de la última vez ya no están
}
session-unreadable = No se pudieron leer los archivos de la última vez, así que no hay ninguno abierto; su lista se guardó aparte
files-unreadable = { $count ->
    [one] { $count } archivo de la última vez no se pudo leer y se omite; la lista tal como estaba se guardó aparte
    [many] { $count } archivos de la última vez no se pudieron leer y se omiten; la lista tal como estaba se guardó aparte
   *[other] { $count } archivos de la última vez no se pudieron leer y se omiten; la lista tal como estaba se guardó aparte
}
session-unsaved = No se pudieron guardar los archivos abiertos para la próxima vez
nothing-to-paste = Nada que pegar
    .why = El portapapeles no tiene archivos, rutas de archivo ni imágenes
paste-failed = No se pudo leer el portapapeles
    .image = No se pudo guardar la imagen pegada
pick-failed = El diálogo de archivos se cerró inesperadamente, así que no se eligió nada


## Dropping files

drop-to-add = Soltar para añadir archivos


## The system's file dialogs: their titles, which say what each is for,
## and the names of the kinds of files they show

dialog-add-files = Añadir archivos para convertir
dialog-add-folder = Añadir una carpeta de fotogramas
dialog-add-images = Añadir imágenes como una sola animación
dialog-inspect = Elegir un sticker para inspeccionar
dialog-ffmpeg = Elegir ffmpeg
dialog-results-folder = Elegir dónde guardar los resultados
images-filter = Imágenes
stickers-filter = Stickers


## The list of files, on the left

settings = Ajustes
light-theme = Cambiar al tema claro
dark-theme = Cambiar al tema oscuro
main-menu = Ajustes y más
menu-undo = Deshacer
    .hint = El último cambio en los ajustes del archivo, o los archivos recién quitados
menu-redo = Rehacer
    .hint = El último cambio deshecho
menu-settings = Ajustes…
inspect = Inspeccionar un sticker…
    .hint = Comprobar si un archivo cumple los límites de Telegram
feedback = Enviar comentarios
    .hint = Escribir al autor en Telegram: en su app, si está instalada, o en el navegador
about-and-licences = Acerca de y licencias…
show-input-in-folder = Mostrar en carpeta
    .pasted = Una imagen pegada no tiene un archivo propio
shortcuts = Atajos de teclado
add-files = Añadir archivos…
add-more = Más formas de añadir archivos
add-folder = Una carpeta de fotogramas…
    .hint = Por orden de nombre, como una sola animación
add-images = Imágenes como una animación…
    .hint = Las imágenes elegidas, por orden de nombre
add-pasted = Del portapapeles
    .hint = Archivos copiados, rutas o una imagen
convert-all = { $count ->
    [0] Convertir todos
   *[other] Convertir todos ({ $count })
}
    .hint = Convertir todos los archivos que no tengan un resultado actualizado
remove-all = Quitar todos
converting-batch = Convirtiendo { $current } de { $total }
stop-all = Detener todo
    .hint = Dejar de convertir y sacar de la cola los archivos en espera
remove = Quitar
remove-file = Quitarlo de la lista
convert = Convertir
    .hint = Crear el sticker o emoji
row-convert-again = Desactualizado: convertir de nuevo
row-next = siguiente
row-waiting = en espera, { $ahead } por delante
row-stale = { $size } · desactualizado
row-failed = error
row-exists = otro archivo ocupa su lugar
row-stopped = detenido
drop-here = Arrastrar aquí videos, GIF, imágenes o archivos de Aseprite
drop-here-note = Para convertirlos en un sticker o emoji de video, animado o estático
paste = Pegar
name-pasted = imagen pegada
name-frames = { $name } (fotogramas)
name-and-more = { $name } y { $more } más
no-font = { $count ->
    [1] No se encontró ninguna fuente para { $codes }, ni entre las de tgradish ni entre las del sistema, así que se muestra por su código. El archivo en sí está bien
   *[other] No se encontró ninguna fuente para { $codes }, ni entre las de tgradish ni entre las del sistema, así que se muestran por sus códigos. El archivo en sí está bien
}


## The bar under the settings of the file picked

stop = Detener
stopping = Deteniendo…
keep-best = Conservar la mejor
    .hint = Dejar de probar codificaciones y guardar como resultado la mejor que cabe hasta ahora
    .nothing-fits = Aún no cabe nada: no hay ninguna codificación que conservar
keeping-best = Conservando la mejor…
cancel-waiting = Cancelar
next-to-convert = Siguiente en la cola
waiting-for = Esperando a { $ahead } más
convert-again = Convertir de nuevo
    .changed = Desactualizado: volver a crearlo con los ajustes actuales
    .hint = Volver a crearlo con los mismos ajustes
-save-settings = Guardar ajustes actuales como preajuste
save-preset = { -save-settings }…
    .hint = Conservarlos como un preajuste propio, del que pueda partir cualquier archivo
failed = Error: { $error }
    .short = Error: ver detalles
in-the-way = Un archivo ajeno a tgradish ocupa su lugar:
replace-it = Reemplazarlo
stopped = Detenido
show-in-folder = Mostrar en carpeta
saves-as = Se guarda como
    .hint = La carpeta de los resultados se elige en «{ settings }»
result-saved-as = Guardado como
copy-result = Copiar
    .hint = Copiar el sticker como archivo, para pegarlo en un chat de Telegram
    .done = Copiado
    .failed = No se copió
replaces-earlier = Reemplaza un resultado anterior
    .any = Reemplaza el archivo existente


## Reading a file to show it

reading-crashed = tgradish falló al leer esto
reading-stopped = Se detuvo la lectura
no-frames = ffmpeg no encontró fotogramas en este archivo
no-frames-there = ffmpeg no encontró fotogramas ahí
no-frame-there = ffmpeg no encontró ningún fotograma ahí
art-no-frames = No tiene fotogramas
result-no-frames = El resultado no tiene fotogramas


## The result's small picture over the preview

inset-label = Resultado ({ $scale })
    .plain = Resultado
    .drag = Arrastrarlo a otra esquina
inset-scale = { $percent }%
    .times = { $times }×
inset-smaller = Más pequeño
    .same = No se vería más pequeño
inset-larger = Más grande
    .same = No se vería más grande


## Over the preview

show-input = Entrada
show-result = Resultado
    .outdated = Resultado ⚠
    .hint = Desactualizado: el resultado se creó con ajustes anteriores
convert-first = Primero hay que convertirlo
tgs-unseen = Este sticker TGS tiene formas que tgradish no puede dibujar
result-preview = Miniatura
    .hint = Mostrar u ocultar una vista previa pequeña junto a la entrada
    .input-only = Se muestra junto a la entrada, no junto al resultado
zoom-out = Alejar
    .hint = Mostrarlo entero
    .all = Ya está alejado al máximo
input-hint = Arrastrar para recortar · rueda para hacer zoom · botón derecho para desplazar
crop-hint = Arrastrar el recorte o sus bordes · doble clic para todo
result-hint = Rueda para hacer zoom · botón derecho para desplazar
unreadable = No se pudo leer: { $error }
reading = Leyendo…
preview-divider = Línea bajo la vista previa


## The timeline under the preview

play = Reproducir
    .pause = Pausar
play-keys =
    Espacio — reproducir o pausar
    ← / → — un fotograma atrás o adelante
    Mayús+← / → — diez fotogramas atrás o adelante
    Inicio / Fin — el primer o el último fotograma usado
    {"["} / ] — empezar o terminar la parte usada en el fotograma mostrado
    Ctrl+← / → — mover el inicio de la parte usada
    Alt+← / → — mover el final de la parte usada
nothing-to-play = Se usa un solo fotograma: no hay nada que reproducir
in-seconds = Segundos
    .hint = Tiempos en segundos
in-frames = Fotogramas
    .hint = Tiempos como números de fotograma, desde 1
    .unknown = No se sabe dónde están los fotogramas de la entrada
part-start = Dónde empieza la parte usada
    .frame = Dónde empieza la parte usada: su primer fotograma
part-end = Dónde termina la parte usada
    .frame = Dónde termina la parte usada: su último fotograma
frame-number = fotograma { $number }
limit-mark = Límite de 3 segundos de Telegram: la parte usada se corta a partir de aquí
    .spoofed = Límite de 3 segundos de Telegram: la parte usada también se reproduce más allá, porque a Telegram se le indica que el sticker es más corto
    .sped-up = Límite de 3 segundos de Telegram: una parte usada más larga se reproduce más rápido para caber en él
frame-before = Fotograma anterior (←)
    .first = Este es el primer fotograma
frame-after = Fotograma siguiente (→)
    .last = Este es el último fotograma
frame-picked = Fotograma


## What to make, beside the preview

make = Crear
format-video = Video
    .hint = Un sticker o emoji de video: VP9 en WebM, a partir de cualquier video o imagen
format-animation = Animado
    .hint = Un sticker o emoji animado: archivo TGS. Esta opción está pensada para pixel art: TGS es un formato vectorial, así que se ve nítido a cualquier tamaño. Ideal para emojis personalizados, que miden solo 100 × 100 px. No sirve para videos ni fotos normales: para eso, usar «{ format-video }»
format-image = Estático
    .hint = Un sticker o emoji estático: WebP, a partir de cualquier imagen o de un fotograma de un video
target-sticker = Sticker
target-emoji = Emoji personalizado
needs-webm-sticker = WebM, 512 px en el lado más largo, hasta 256 KiB
needs-webm-emoji = WebM, 100 × 100 px, hasta 64 KiB
needs-tgs = TGS, cuadrado, hasta 64 KiB
needs-image-sticker = WebP, 512 px en el lado más largo, hasta 512 KiB
needs-image-emoji = WebP, 100 × 100 px, hasta 512 KiB
not-pixel-art = «{ format-animation }» está pensado para pixel art, que se convierte a vectores para que se vea nítido. Esto no parece pixel art: le va mejor «{ format-video }»
preset = Preajuste
    .hint = Cuánto esfuerzo dedicar al resultado; los valores propios de abajo se aplican encima
    .image = Un sticker estático solo toma del preajuste los ajustes de imagen, es decir, cómo se hace cuadrado y cómo se escala: se guarda sin pérdida si cabe y, si no, con la mejor calidad con la que aún quepa
    .broken = El archivo de este preajuste está dañado
    .missing = Ya no existe el preajuste «{ $name }»: elegir otro
manage-presets = Gestionar…
    .hint = Renombrar, describir o eliminar los preajustes propios
preset-fast = Rápido
    .webm = Solo ajusta la tasa de bits, con el codificador rápido: tarda segundos, pero queda más borroso
    .tgs = Tarda menos de un segundo; ocupa un pequeño porcentaje más
preset-balanced = Equilibrado
    .webm = Ajusta la tasa de bits y prueba frecuencias de fotogramas más bajas donde podrían verse mejor
    .tgs = Entre rápido y óptimo
preset-best = Óptimo
    .webm = Como equilibrado, con el codificador más lento: tarda minutos con videos largos
    .tgs = Tarda unos segundos; el archivo más pequeño
preset-described = { $name }: { $description }


## Settings: the picture

picture = Imagen
picture-size = Tamaño
once-read = Se muestra cuando se lea la entrada
crop = Recorte
    .hint = La parte de la entrada que se usa, en sus píxeles. Arrastrar en la vista previa para cambiarla
margin-top = Margen superior: píxeles que se cortan arriba
margin-left = Margen izquierdo: píxeles que se cortan a la izquierda
margin-right = Margen derecho: píxeles que se cortan a la derecha
margin-bottom = Margen inferior: píxeles que se cortan abajo
width = Ancho
height = Alto
crop-keys = Recorte: las flechas lo mueven y Ctrl+flechas lo redimensiona; con Mayús, de diez en diez píxeles
whole-picture = Imagen completa
    .hint = Usar toda la imagen
    .exact = Usar toda la imagen, o tanto como abarque la escala fija desde su esquina superior izquierda
middle = Centro
    .hint = Usar tanto del centro como abarque la escala fija
aspect-ratio = Relación de aspecto
    .hint = La proporción entre ancho y alto que mantiene el recorte al arrastrarlo
    .square = TGS debe ser cuadrado y «{ square }» está en «{ square-off }», que mantiene la forma del recorte: elegir esto vuelve a hacerlo cuadrado
shape-free = Libre
    .hint = Cualquier forma
shape-preserve = Original
    .hint = La forma propia de la entrada
shape-custom = Personalizada
    .hint = Una forma escrita a mano: como proporción o como tamaño del resultado
    .pick = Elegir «{ shape-custom }» para escribir una forma
ratio-result = Resultado
    .fixed = El resultado se hace cuadrado o tiene márgenes, así que su tamaño no sigue al recorte
    .width = Ancho del resultado
    .height = Alto del resultado
square = Forzar cuadrado
    .hint = Cómo se hace cuadrado el resultado
    .tgs = Cómo se hace cuadrado el pixel art (como deben serlo todos los TGS)
square-off = No
    .hint = El resultado mantiene la forma del recorte
    .emoji = Los emojis deben ser cuadrados
    .tgs = Los TGS deben ser cuadrados, y también el recorte que mantienen: elegir esto pone «{ aspect-ratio }» en 1:1
    .kept = El pixel art mantiene la forma del recorte, que «{ aspect-ratio }» mantiene cuadrada (como deben serlo todos los TGS)
square-pad = Rellenando
    .hint = Márgenes transparentes llenan el resto del cuadrado
    .tgs = Márgenes transparentes llenan el resto del cuadrado
square-crop = Recortando
    .hint = El centro llena el cuadrado y el resto se corta: la vista previa lo atenúa
    .tgs = El centro llena el cuadrado y el resto se corta
square-stretch = Estirando
    .hint = La imagen se estira hasta llenar el cuadrado
    .tgs = El pixel art se estira hasta llenar el cuadrado (sus píxeles dejan de ser cuadrados)
square-exact = La escala entera fija conserva toda la imagen: usar relleno en su lugar
scaling = Escalado
    .hint = Cómo se convierten los píxeles de la entrada en los del resultado
scaling-auto = Automático
    .hint = Píxeles nítidos para pixel art ampliado al doble o más; suave en otro caso
scaling-smooth = Suave
    .hint = Los píxeles vecinos se mezclan
scaling-sharp = Píxeles nítidos
    .hint = Cada píxel se convierte en un bloque, y los bloques solo se mezclan en sus bordes
scaling-integer = Entero
    .hint = Cada píxel se convierte en el mismo número entero de píxeles del resultado, o bloques enteros de ellos se convierten en uno; márgenes transparentes llenan el resto
integer-scale = Escala entera
    .hint = En cuántos píxeles del resultado se convierte cada píxel de la entrada, o cuántos píxeles de la entrada forman uno
    .only = Solo con escalado «{ scaling-integer }»
scale-largest = La mayor que quepa
    .hint = El mayor múltiplo entero que mantiene el recorte dentro del resultado
    .preset = El preajuste fija una escala
scale-fixed = Fija
    .hint = Esta escala; el recorte puede ser, como mucho, lo que con ella cabe en el resultado
smaller = Menor
larger = Mayor
scale-field = Escribir 3 para el triple de tamaño, o 1/3 para un tercio
    .pick = Elegir «{ scale-fixed }» para indicar una escala
scale-unused = Las escalas enteras solo se aplican con escalado «{ scaling-integer }»
scale-unsuited = El recorte aún no encaja con esta escala
scale-makes = { $scale } deja el recorte en { $width } × { $height } px
scale-makes-padded = { $scale } deja el recorte en { $width } × { $height } px; márgenes transparentes llenan el resto de los { $box_width } × { $box_height }
scale-each-way = ×{ $across } en horizontal y ×{ $down } en vertical
scale-shrinks = El recorte es mayor que el resultado, así que se reduce con píxeles nítidos: una escala fija como ×1/2 lo reduce por bloques enteros
margins = Márgenes
    .hint = Filas y columnas del borde que son transparentes en todos los fotogramas de la entrada
auto-crop = Recortar márgenes vacíos automáticamente
    .hint = Recorta el pixel art al menor rectángulo con píxeles visibles en algún fotograma
    .after = Se aplica después del recorte de arriba
art-pixel = Píxel de arte
    .hint = Cuántos píxeles de la entrada ocupa un píxel de arte. Los píxeles que quedan fuera de su cuadrícula se mueven a ella
art-pixel-detected = Detectado
    .hint = Hallado a partir del pixel art: sus bloques más pequeños de un solo color


## Settings: time

time = Tiempo
used-part = Parte usada
    .hint = Dónde empieza y termina la parte de la entrada que se usa; la línea de tiempo bajo la vista previa también la define
part-from = Desde
part-to = hasta
part-first = Su primer fotograma
part-last = Su último fotograma
all-of-input = Toda la entrada
part-seconds = { $from } a { $to } de { $length }
part-frame = Fotograma { $number } de { $count }
part-frames = Fotogramas { $first } a { $last } de { $count }
speed = Velocidad
    .hint = Cuántas veces más rápido que la entrada se reproduce el resultado
from-preset = Del preajuste
    .hint = El valor que fija el preajuste
from-input = Como la entrada
    .speed = Tan rápido como se reproduce la entrada
    .rate = La frecuencia de la entrada a esa velocidad, como máximo 60 fps
fits-3-s = Caber en 3 s
    .hint = Tan rápido como haga falta para que la parte usada quepa en 3 segundos
fit-3-s = Ajustar a 3 s
    .hint = Fijar la velocidad para que la parte usada dure 3 segundos
plays = Dura { $played }
plays-cut = Dura { $kept } de los { $used }: el resto se corta
plays-spoofed = Dura los { $played } enteros: a Telegram se le indica que es más corto
frame-rate = Frec. de fotogramas
    .hint = Menos fotogramas por segundo dejan más bytes para cada uno; más que los de la entrada solo los repetirían
    .sequence = A qué velocidad se reproducen los fotogramas de la secuencia
    .sheet = A qué velocidad se reproducen los fotogramas de una secuencia u hoja de sprites
default-rate = Predeterminada
    .hint = { $fps } fps, ya que estos fotogramas no indican a qué velocidad se reproducen
over-3-s = Falsear duración
    .hint = Telegram rechaza los stickers que dicen durar más de 3 segundos
spoof-auto = Si es más largo
    .hint = A los stickers de más de 3 segundos se les codifica una duración corta: Telegram los acepta y los reproduce enteros
spoof-always = Siempre
    .hint = Incluso a los stickers cortos se les codifica la duración corta
spoof-never = Nunca: cortar a los 3 s
    .hint = No falsea los metadatos y corta el video a los 3 segundos
spoof-to = Duración falseada
    .hint = La duración codificada en los metadatos, que es la que lee Telegram
    .honest = Con «{ spoof-never }» elegido, no se falsea nada
default-duration = Predeterminada
    .hint = Una duración corta que Telegram acepta
frame = Fotograma
frame-at = Fotograma en
    .hint = El momento del fotograma del que se hace la imagen; la línea de tiempo bajo la vista previa también lo elige


## Settings: size and quality of a WebM

size-and-quality = Tamaño y calidad
fitting = Ajuste al límite
    .hint = Qué cambia tgradish para quedar justo por debajo del límite de tamaño de archivo
    .lossless = La codificación sin pérdida no tiene tasa de bits ni calidad que ajustar: elegir esto desmarca «{ lossless-encoding }»
fit-auto = Mejor aspecto
    .hint = Ajusta la tasa de bits y, si nada cabe o si menos fotogramas, cada uno con más bits, podrían verse mejor, prueba frecuencias de fotogramas más bajas y se queda con lo que más se parece a la entrada
fit-bitrate = Tasa de bits
    .hint = Ajusta la tasa de bits a la frecuencia de fotogramas elegida
fit-crf = Calidad
    .hint = Ajusta el valor de calidad constante (CRF)
fit-fps = FPS
    .hint = Ajusta la frecuencia de fotogramas con calidad constante
fit-length = Duración
    .hint = Ajusta la duración con calidad constante, cortando el final
fit-off = Sin ajuste
    .hint = Codifica una vez con los valores de abajo, aunque quede demasiado grande
encoder = Esfuerzo al codificar
    .hint = Una codificación más rápida se ve peor con el mismo tamaño
encoder-fast = Rápido
    .hint = Tarda segundos, pero se ve claramente peor con el mismo tamaño
encoder-balanced = Equilibrado
encoder-best = Óptimo
    .hint = Lento: tarda minutos con videos largos
fitting-off = «{ fit-off }» codifica una sola vez
attempts = Codificar hasta
    .hint = Cuántas veces puede codificar el ajuste al límite, como máximo
attempts-default = Predeterminado
    .hint = Suficiente para quedar a pocos puntos porcentuales del límite de tamaño de archivo con la mayoría de las entradas
attempts-times = { $count ->
    [one] vez
    [many] veces
   *[other] veces
}
search-within = Buscar entre
    .hint = El rango en el que busca el ajuste al límite, en la unidad del valor ajustado
    .low = Mínimo
    .high = Máximo
search-auto = Automático
    .hint = Tan amplio como convenga al modo de ajuste y a la entrada
range-to = y
crf = Calidad (CRF)
    .hint = Calidad constante de 0, la mejor, a 63
    .lossless = La codificación sin pérdida no usa ninguna calidad
    .fitted = El ajuste al límite encuentra la calidad
    .bitrate = El ajuste al límite encuentra una tasa de bits, no una calidad
crf-default = Predeterminada
    .hint = Una buena calidad para el tamaño de los stickers
lossless = Sin pérdida
lossless-encoding = Codificación sin pérdida
    .hint = Solo es pequeña con imágenes diminutas o fijas
    .switches = Solo es pequeña con imágenes diminutas o fijas. La codificación sin pérdida no tiene tasa de bits ni calidad que ajustar: marcar esto cambia el ajuste al límite a «{ fit-fps }»
bitrate = Tasa de bits
    .hint = Se usa salvo que solo se fije una calidad
    .fitted = El ajuste al límite encuentra la tasa de bits
    .constant = Se codifica con calidad constante, no con una tasa de bits
bitrate-auto = Según el límite de tamaño
    .hint = Tanta como quepa según la duración


## Settings: size and quality of a TGS animation

tgs-encoder = Esfuerzo al codificar
    .hint = Cuánto esforzarse en buscar una codificación pequeña
tgs-fast = Rápido
    .hint = Tarda menos de un segundo; ocupa un pequeño porcentaje más
tgs-balanced = Equilibrado
tgs-best = Óptimo
    .hint = Tarda unos segundos; el archivo más pequeño
too-large = Si no cabe
    .hint = Qué pasa cuando el pixel art no cabe en 64 KiB tal como está
never-change = No cambiar nunca el pixel art; avisar en su lugar
    .set = El pixel art no se cambia nunca, según lo elegido arriba
give-up = Sacrificar primero
    .hint = Cuando el pixel art tiene que cambiar para caber: su movimiento o su detalle
give-up-either = Cualquiera
    .hint = Lo que menos cambie el pixel art por los bytes que ahorra
give-up-motion = Movimiento
    .hint = Primero se fusionan y descartan fotogramas: todo el detalle, movimiento más entrecortado
give-up-detail = Detalle
    .hint = Primero la imagen pierde detalle: movimiento fluido, menos detalle
ways-to-fit = Cómo hacerlo caber
    .hint = Lo que el ajuste al límite puede hacer con el pixel art para que quepa en 64 KiB, de lo menos a lo más visible
reduce-snap = Ajustar a la cuadrícula de píxeles
    .hint = Mueve los píxeles desplazados a la cuadrícula de píxeles de arte
reduce-colours = Fusionar colores parecidos
    .hint = Los colores que apenas difieren pasan a ser uno
reduce-specks = Quitar motas
    .hint = Píxeles sueltos que difieren de todos los que los rodean
reduce-frames = Fusionar fotogramas parecidos
    .hint = Los fotogramas que apenas difieren muestran una sola imagen
reduce-drop = Descartar fotogramas
    .hint = Muestra uno de cada dos fotogramas, o menos
reduce-resolution = Bajar la resolución
    .hint = Menos píxeles de arte, y más grandes
reading-art = Lectura del pixel art
sprite-sheet = Hoja de sprites
    .hint = Fotogramas dispuestos en una cuadrícula de celdas iguales, fila por fila
read-as-sheet = Leer como hoja de sprites
sheet-refused = Este archivo ya tiene { $frames } fotogramas: una hoja de sprites es una sola imagen
    .converted = Se leyó tal cual, no como hoja de sprites: ya tiene { $frames } fotogramas
sheet-cells = Celdas
    .columns = Columnas
    .rows = Filas
sheet-frames = Fotogramas
sheet-all = Todos
    .hint = Todas las celdas salvo las transparentes del final


## Settings: an Aseprite file's tags and layers

aseprite-file = Archivo de Aseprite
unreadable-choices = No se puede leer con la etiqueta y las capas elegidas
read-all = Leerlo entero
tag = Etiqueta
    .hint = Toma los fotogramas de una etiqueta: según dónde empieza y termina, o leídos en su propio orden si se reproduce al revés o de ida y vuelta
all-frames = Todos los fotogramas
    .hint = Todos los fotogramas del archivo
tag-frame = Fotograma { $number }
tag-frames = Fotogramas { $from } a { $to }
tag-reverse = { $frames }, al revés
tag-ping-pong = { $frames }, de ida y vuelta
tag-pong-ping = { $frames }, de vuelta e ida
layers = Capas
    .hint = Qué capas se muestran, la de arriba primero; un grupo oculto oculta las capas que contiene
    .group-hidden = Su grupo está oculto
    .file = Volver a las capas que muestra el archivo


## Settings: encoder options, folded away under the rest

encoder-options = Opciones del codificador
vp9-options = Opciones del codificador por nombre, como tune-content = screen para gráficos planos. ffmpeg -h encoder=libvpx-vp9 las enumera
    .name = Por ejemplo: tune-content
    .value = Por ejemplo: screen
add-option = Añadir una opción
examples = Ejemplos:
example-tune-content = Gráficos planos y grabaciones de pantalla: bordes más nítidos
example-aq-mode = Asigna más bits donde la imagen es compleja
example-sharpness = Conserva más detalle fino, a costa de algo de suavidad
example-arnr-strength = Menos reducción de ruido entre fotogramas
example-g = Un fotograma clave al menos cada 60 fotogramas
example-qmax = Nunca peor que la calidad 50 en ningún punto
ffmpeg-arguments = Argumentos ffmpeg
    .hint = Argumentos sin procesar que se añaden antes de la salida, separados como lo haría un shell
    .placeholder = Por ejemplo: -qmin 4
    .unfinished = Hay una comilla o una barra invertida sin cerrar: los argumentos se quedan como se leyeron por última vez hasta que se cierre
    .system = Solo con el ffmpeg del sistema, que se puede elegir en «{ settings }»


## What keeps a file from being converted as it is, and the ways out

blocked = Aún no se puede convertir: { $reason }
    .heading = Aún no se puede convertir
blocked-crop = el recorte se sale de la imagen
blocked-sheet = las celdas de la hoja de sprites no dividen la imagen
blocked-tag = el archivo no tiene la etiqueta «{ $tag }»
blocked-layers = { $count ->
    [1] el archivo no tiene la capa { $layers }
   *[other] el archivo no tiene las capas { $layers }
}
blocked-ffmpeg = no se encontró ffmpeg
crop-outside = El recorte, de { $width } × { $height } desde { $left }, { $top }, se sale de esta imagen de { $across } × { $down }
use-whole = Usar toda la imagen
    .hint = No recortar nada y conservar toda la imagen
sheet-uneven = La imagen, de { $width } × { $height }, no se divide en { $columns } × { $rows } celdas iguales
sheet-nearest = Lo más cercano que divide: { $columns } × { $rows }
    .hint = La distribución más cercana a esta cuyas celdas dividen la imagen
tag-missing = Este archivo no tiene la etiqueta «{ $tag }»
layers-missing = { $count ->
    [1] Este archivo no tiene la capa { $layers }
   *[other] Este archivo no tiene las capas { $layers }
}
layers-as-file = Mostrar capas como en el archivo
    .hint = Mostrar y ocultar las capas como lo hace el archivo
needs-ffmpeg = Esto necesita ffmpeg, que no se encontró
    .hint = Instalar ffmpeg desde los paquetes del sistema o desde ffmpeg.org, o elegir dónde está. Los archivos se vuelven a leer en cuanto se encuentra
choose-ffmpeg = Elegir ffmpeg…
    .hint = Elegir el programa ffmpeg, o una carpeta con ffmpeg y ffprobe
row-attention = requiere atención
    .hint = Algo impide convertirlo: ver sus ajustes
row-needs-ffmpeg = necesita ffmpeg
    .hint = ffmpeg lo lee o crea su resultado, y no se encontró
need-attention = { $count ->
    [one] { $count } requiere atención
    [many] { $count } requieren atención
   *[other] { $count } requieren atención
}


## What converting will do, beside the preview, step by step

plan-video = Qué se hará con el video:
plan-picture = Qué se hará con la imagen:
plan-animation = Qué se hará con la animación:
plan-art = Qué se hará con el pixel art:
plan-video-frame = Qué se hará con un fotograma del video:
plan-animation-frame = Qué se hará con un fotograma de la animación:
plan-art-frame = Qué se hará con un fotograma del pixel art:
plan-tag = Se reproduce según su etiqueta «{ $tag }»
plan-tag-reverse = Se reproduce según su etiqueta «{ $tag }», al revés
plan-tag-ping-pong = Se reproduce según su etiqueta «{ $tag }», de ida y vuelta
plan-tag-pong-ping = Se reproduce según su etiqueta «{ $tag }», de vuelta e ida
plan-cropped = Se recorta a { $width } × { $height }
plan-frame = Se toma el fotograma en { $time }
plan-scaled = Se escala a { $width } × { $height }
plan-stretched-across = Se estira ×{ $factor } en horizontal
plan-stretched-down = Se estira ×{ $factor } en vertical
plan-middle-cut = Se recorta el centro a { $width } × { $height }
plan-padded = Se rellena hasta { $width } × { $height }
plan-trimmed = Se recorta a sus píxeles visibles
plan-grid = Se alinean los píxeles a una cuadrícula de { $scale } × { $scale }
plan-square-crop = Se recorta a un cuadrado
plan-square-stretch = Se estira a un cuadrado
plan-square-pad = Se rellena hasta un cuadrado
plan-cut = Se corta a { $from }–{ $to } de { $length }
plan-faster = Se acelera ×{ $factor }
plan-slower = Se ralentiza ×{ $factor }
plan-cut-3-s = Se corta a los 3 s de { $played }
plan-spoofed = Se falsea la duración, para que se reproduzcan los { $played } enteros
plan-spoofed-anyway = Se falsea la duración, aunque { $played } ya es lo bastante corto
plan-saved = Se guarda como { $name }


## Settings: the same conversion as commands, folded away under the rest

commands = Comandos
copy = Copiar
commands-posix = La misma conversión en la línea de comandos, para un shell POSIX:
commands-powershell = La misma conversión en la línea de comandos, para PowerShell:
pasted-temporary = Una imagen pegada se guarda en una carpeta temporal, que desaparece al cerrar tgradish
ffmpeg-commands = Los comandos de ffmpeg de la primera codificación; el ajuste al límite cambia la tasa, y quizá la frecuencia de fotogramas, entre codificaciones:
    .builtin = Lo que hace el ffmpeg integrado en la primera codificación, como comandos para el ffmpeg del sistema; el ajuste al límite cambia la tasa, y quizá la frecuencia de fotogramas, entre codificaciones:
no-ffmpeg-commands = Sin comandos de ffmpeg: { $why }
    .unread = la entrada aún no se ha leído
    .art = ffmpeg lee un PNG del pixel art que dibuja cada conversión


## The Settings dialog

save = Guardar
saved-to = Guardado en { $path }
    .kept = Guardado en { $path }; el archivo que no se podía leer se conserva como { $kept }
not-saved = No se pudo guardar: { $error }
no-home = No hay carpeta personal donde guardar los ajustes
settings-note = Los cambios se aplican y se guardan al hacerlos
reset-settings = Restablecer
    .hint = Devolver cada ajuste de aquí a como lo tenía tgradish al principio
ffmpeg-given = Esta ejecución usa el ffmpeg indicado al iniciar tgradish; estos ajustes se aplican cuando se inicia sin él
ffmpeg-use = Usar
    .hint = Los videos se leen, y los stickers WebM se crean, con ffmpeg
ffmpeg-auto = Automático
    .hint = El ffmpeg integrado si esta compilación lo tiene; si no, el del sistema
ffmpeg-builtin = Integrado
    .hint = El ffmpeg que viene dentro de tgradish
    .missing = Esta compilación de tgradish no incluye ffmpeg
ffmpeg-system = Del sistema
    .hint = ffmpeg y ffprobe en el PATH; admite argumentos de ffmpeg sin procesar
ffmpeg-program = Programa
    .hint = Un ejecutable de ffmpeg, o una carpeta con ffmpeg y ffprobe; prevalece sobre la opción de arriba
    .missing = Ahí no hay nada, así que el programa en uso sigue siendo el mismo
ffmpeg-found = { $path }, encontrado en el PATH
for-example = Por ejemplo: { $example }
choose = Elegir…
check = Comprobar
    .hint = Comprobar si este ffmpeg puede crear stickers
checking-ffmpeg = Comprobando…
ffmpeg-ready = ffmpeg { $version } con VP9: listo
ffmpeg-no-vp9 = ffmpeg { $version } no puede codificar VP9 con libvpx
converting-settings = Conversión
threads = Hilos
    .hint = Cuántos usa a la vez la conversión, como máximo; con menos queda más equipo libre para lo demás, pero se tarda más
threads-auto = Automático
    .hint = Tantos como aproveche cada parte de la conversión
results = Resultados
save-results = Guardarlos
next-to-input = Junto a cada entrada
in-a-folder = En una carpeta
    .unchosen = Hasta que se elija una carpeta, los resultados se guardan junto a cada entrada
    .relative = Escribir la ruta completa, desde la raíz, para cambiar la carpeta en uso
    .file = Eso es un archivo, no una carpeta, así que la carpeta en uso sigue siendo la misma
existing-files = Archivos existentes
    .hint = Los resultados creados por tgradish siempre se reemplazan al volver a crearlos
replace-in-the-way = Reemplazar cualquier archivo que ocupe su lugar
interface = Interfaz
language = Idioma
    .hint = El idioma que se muestra aquí; la línea de comandos sigue en inglés
    .variable = Elegido por la variable de entorno { $variable }
as-the-system = Según el sistema
as-the-system-language = Según el sistema: { $language }
theme = Tema
    .hint = El botón junto a ⚙ también cambia entre claro y oscuro
theme-light = Claro
theme-dark = Oscuro
accent = Color de acento
    .hint = Marca lo elegido, lo que está bajo el puntero y lo que tiene el foco
scale = Escala
    .hint = El tamaño del texto y de los controles
    .keys = { $larger } y { $smaller } también la cambian; { $reset } la devuelve al 100%
scale-smaller = Reducir
scale-larger = Ampliar
scale-reset = Volver al 100%
scale-percent = { $percent }%
accent-system = { as-the-system }
    .hint = El color de acento del escritorio, o el azul de Breeze si no tiene
accent-pink = Rosa
accent-red = Rojo
accent-orange = Naranja
accent-yellow = Amarillo
accent-green = Verde
accent-teal = Verde azulado
accent-blue = Azul
accent-purple = Morado
accent-violet = Violeta
accent-grey = Gris
accent-own = Un color propio
    .chosen = Un color propio, { $colour }
accent-shade = Saturación y brillo
    .value = Saturación { $saturation }%, brillo { $value }%
accent-hue = Matiz
accent-hex = Hex
    .wrong = Escribirlo como # y seis dígitos hexadecimales, como #3daee9
files = Archivos
reopen-files = Volver a abrir los archivos de la última vez, con sus ajustes
motion = Animaciones
    .hint = Si el sistema pide menos movimiento, nada se mueve en ningún caso
motion-on = Suavizar el desplazamiento, y desvanecer y mover lo que cambia
new-files = Archivos nuevos
preset-for-webm = Preajuste para WebM
preset-for-tgs = Preajuste para TGS


## The result, beside the preview

last-conversion = Última conversión
path-in-the-way = { $path } ocupa su lugar
stopped-early = Se detuvo antes de terminar
result = Resultado
    .stale = Resultado, desactualizado
result-copy = Copiarlo ({ $keys }) para pegarlo en un chat de Telegram
result-webm = { $width } × { $height } px, { $fps } fps, { $length }
    .spoofed = { $width } × { $height } px, { $fps } fps, { $length }, duración falseada
changed-to-fit = Cambiado para caber; abajo se detalla cómo
replaced-earlier = Reemplazó un resultado anterior creado por tgradish
converting = Convirtiendo
stage-reading = Leyendo el pixel art
stage-drawing = Convirtiendo a vectores
stage-fitting-steps = { $count ->
    [one] Ajustando a 64 KiB: { $count } paso, unos { $size }
    [many] Ajustando a 64 KiB: { $count } pasos, unos { $size }
   *[other] Ajustando a 64 KiB: { $count } pasos, unos { $size }
}
stage-fitting-from = Ajustando a 64 KiB desde { $size }
stage-compressing = Comprimiendo
    .hopeless = Compresión rápida: no puede caber
    .missed = Comprimido → { $size }, demasiado grande
    .missed-far = Comprimiendo: aún demasiado grande
retry = Intento { $number }: objetivo { $aim }
retry-done = Intento { $number }: objetivo { $aim } → { $size }, demasiado grande
    .far = Intento { $number }: objetivo { $aim } → aún demasiado grande
starting = Iniciando
encode = Codificación { $number }: { $fps } fps, { $rate }
encode-of = Codificación { $number } de hasta { $most }: { $fps } fps, { $rate }
encode-done = Codificación { $number }: { $fps } fps, { $rate } → { $size }, cabe
    .too-large = Codificación { $number }: { $fps } fps, { $rate } → { $size }, demasiado grande
rate-quality = calidad { $crf }
rate-lossless = sin pérdida
encodes-up-to = { $count ->
    [one] Como máximo { $count } codificación, hasta que una quede justo por debajo del límite de tamaño de archivo
    [many] Como máximo { $count } codificaciones, hasta que una quede justo por debajo del límite de tamaño de archivo
   *[other] Como máximo { $count } codificaciones, hasta que una quede justo por debajo del límite de tamaño de archivo
}
art-size = { $width } × { $height } píxeles de arte
    .scaled = { $width } × { $height } píxeles de arte, cada uno de { $scale } × { $scale } píxeles de la entrada
art-summary = { $size }; { $colours }, { $frames }
art-tempo = Se reproduce a ×{ $tempo } la velocidad de la entrada
art-off-grid = Casi todo parece pixel art a { $scale }× con algunos píxeles fuera de la cuadrícula; un tamaño de píxel de arte de { $scale } los ajusta a ella
art-shapes = { $layers } con { $rectangles }, { $size } sin comprimir
art-before-fitting = Unos { $size } tal como está; cambios para caber:
art-step = { $change }, unos { $size } después
step-snapped = píxeles ajustados a una cuadrícula de { $scale }
step-colours = colores a menos de un { $distance }% fusionados
step-frames = fotogramas que cambian menos de un { $changed }% fusionados
step-dropped = { $share }% de los fotogramas descartados
step-specks = píxeles sueltos eliminados
step-resolution = resolución reducida al { $factor }%
column-number = Cada codificación por orden; ✔ la conservada
column-fps = Fotogramas por segundo
column-rate = Tasa de bits, o la calidad con la que se codificó
column-size = tamaño
    .hint = Proporción del límite de tamaño de archivo, en verde si cabe
column-alike = parecido
    .hint = Cuánto se parece a la entrada, medido con SSIM (similitud estructural)
rate-quality-short = q { $crf }
decided-in-band = Detenido: la codificación { $number } usa el { $share } del límite, al menos el { $least } buscado
decided-alike-enough = Detenido: la codificación { $number } ya se parece un { $alike } a la entrada, así que más bits solo la harían más grande
decided-alike-unchanged = Detenido: la codificación { $number } se parece un { $alike } a la entrada, apenas más que la codificación { $than } con menos bits
decided-little-to-gain = Detenido: la codificación { $number } usa el { $share } del límite, y otra usaría solo alrededor del { $predicted }
decided-too-large-at-least = Detenido: incluso el mínimo del rango, en la codificación { $number }, usa el { $share } del límite
decided-nothing-left = Detenido: no queda ningún ajuste que valga la pena probar
decided-out-of-attempts = { $count ->
    [one] Detenido: se hizo { $count } codificación, la única permitida
    [many] Detenido: se hicieron las { $count } codificaciones permitidas
   *[other] Detenido: se hicieron las { $count } codificaciones permitidas
}
decided-lowers-to-fit = Probando { $fps } fps: nada cupo a { $from } fps
decided-lowers-for-looks = Probando { $fps } fps: la codificación { $number } a { $from } fps solo se parece un { $alike } a la entrada
decided-settled = Detenido antes de tiempo: se conservó la codificación { $number }
decided-lower-no-better = Detenido: a { $fps } fps se parece un { $alike } a la entrada, no más que el { $kept_alike } de la codificación { $kept } a { $kept_fps } fps
issue-not-webm = Su contenedor es { $container }, no WebM
issue-no-video = No tiene pista de video
issue-not-vp9 = Su códec es { $codec }, no VP9
issue-audio = Tiene una pista de audio
issue-size = Mide { $width } × { $height } px, y Telegram admite { $side } × { $side } px
    .sticker = Mide { $width } × { $height } px, y Telegram admite un lado de { $side } px y el otro como mucho igual
issue-too-big = Ocupa { $size }, más que el límite de { $limit }
issue-too-long = Sus metadatos dicen que dura { $seconds }, más que el límite de { $limit }
issue-too-many-fps = Su frecuencia de fotogramas es de { $fps } fps, más que el límite de { $limit } fps
issue-animated = Es animado, y los stickers estáticos son una sola imagen
issue-frame-rate = Su frecuencia de fotogramas es de { $fps } fps, y Telegram admite { $required } fps
    .accepted = Su frecuencia de fotogramas es de { $fps } fps: va contra las reglas de Telegram, aunque Telegram la admite
issue-no-frames = No tiene fotogramas
issue-unpacked = Sin comprimir ocupa { $size }, más que el límite de { $limit } de Telegram Desktop
issue-count =
    .layers = Capas: { $count }, más que el límite de { $limit }
    .shapes-per-layer = Máximo de formas en una capa: { $count }, más que el límite de { $limit }
    .paint-points = Máximo de puntos de trazado en un relleno: { $count }, más que el límite de { $limit }
    .paints-per-layer = Máximo de rellenos y trazos en una capa: { $count }, más que el límite de { $limit } de las apps de Telegram
    .paint-sources = Máximo de formas pintadas en una capa: { $count }, más que el límite de { $limit } de las apps de Telegram
    .path-points = Máximo de puntos en un trazado: { $count }, más que el límite de { $limit } de las apps de Telegram
    .keyframes = Máximo de fotogramas clave en una propiedad: { $count }, más que el límite de { $limit } de las apps de Telegram
    .assets = Recursos: { $count }, más que el límite de { $limit } de las apps de Telegram
    .expanded-layers = Capas con las precomposiciones expandidas: { $count }, más que el límite de { $limit } de las apps de Telegram
    .nesting = Niveles de anidamiento en su JSON: { $count }, más que el límite de { $limit } de las apps de Telegram
    .group-depth = Niveles de grupos dentro de grupos: { $count }, más que el límite de { $limit } de las apps de Telegram
issue-cost = Costo de dibujo: { $cost }, más que el límite de { $limit }; formas: { $shapes }, capas: { $layers }, { $each } cada una
issue-empty-tangents = Trazados con tangentes vacías ([]), que Telegram rechaza: { $count }; [0,0] funciona
issue-path-coordinate = Mayor coordenada de trazado: { $coordinate }, más que el límite de { $limit } de las apps de Telegram
issue-precomp-loop = Sus precomposiciones se refieren unas a otras en un bucle
issue-feature = { $feature }: va contra las reglas de Telegram, aunque Telegram lo admite
    .undrawn = { $feature }: va contra las reglas de Telegram; Telegram lo admite, pero sus apps no lo dibujan
    .ignored = { $feature }: las apps de Telegram no lo dibujan
warned-fewer-frames = Se leyó a { $fps } fps, porque más fotogramas de { $width } × { $height } no cabrían en memoria
warned-many-colours = Tiene { $colours }, algo poco común en el pixel art, así que el sticker será grande
warned-length-capped = Las duraciones que prueba el ajuste se recortan a { $length }, porque el ajuste solo acorta
warned-input-shorter = La entrada dura solo { $length } desde su inicio: se usa entera
warned-trimmed = Cortado a { $length }: el falseo de duración está desactivado y Telegram rechaza videos más largos
warned-left-out = Se escribió sin el nombre de tgradish en los metadatos, porque los argumentos de ffmpeg sin procesar no le dejaron sitio: { $details }
    .duration = Se escribió sin la duración falseada en los metadatos, porque los argumentos de ffmpeg sin procesar no le dejaron sitio: { $details }


## Inspecting a sticker

inspect-another = Inspeccionar otro…
inspect-webm-sticker = Un sticker de video
inspect-webm-emoji = Un emoji de video
inspect-image-sticker = Un sticker estático
inspect-image-emoji = Un emoji estático
inspect-tgs = Una animación vectorial, { $width } × { $height } a { $fps } fps
inspect-unknown = Un tipo de sticker que esta versión no puede mostrar
inspect-size = Tamaño
    .hint = Telegram rechaza archivos más grandes
    .tgs = Telegram rechaza stickers más grandes
inspect-duration = Duración
    .hint = Telegram lee la duración de los metadatos; al falsearla, figura como corta
    .spoofed = { $seconds } s en los metadatos, falseada
    .no-header = { $seconds }, sin duración en los metadatos
inspect-plays = Duración real
    .hint = Lo que dura el video de verdad
inspect-frame-rate = Frecuencia de fotogramas
    .value = { $fps } fps de { $most }, { $frames }
inspect-picture = Imagen
    .none = ninguna
inspect-video = { $width } × { $height } px, { $codec }
    .alpha = { $width } × { $height } px, { $codec }, con transparencia
inspect-webp = { $width } × { $height } px, WebP, sin pérdida
    .alpha = { $width } × { $height } px, WebP, sin pérdida, con transparencia
    .lossy = { $width } × { $height } px, WebP, con pérdida
    .lossy-alpha = { $width } × { $height } px, WebP, con pérdida, con transparencia
inspect-audio = Audio
inspect-title = Título
inspect-made-with = Creado con
    .hint = Los programas que escribieron el archivo, según este
    .image = El programa que nombran sus metadatos
    .unknown = desconocido
inspect-note = Nota
    .hint = Texto en el relleno del archivo, que los reproductores ignoran
inspect-name = Nombre
    .hint = El nombre propio de la animación, que Telegram no muestra
inspect-length = Duración
    .hint = Los stickers animados duran como máximo 3 segundos
    .value = { $seconds } de 3.00 s, { $frames }
inspect-cost = Costo de dibujo
    .hint = Formas más 9 por cada capa, como las cuenta Telegram
inspect-layers = Capas
inspect-shapes = Máximo de formas por capa
inspect-refuses-more = Telegram rechaza más
inspect-points = Máximo de puntos por relleno
    .hint = Puntos de trazado que pinta un relleno; Telegram rechaza más
inspect-unpacked = Sin comprimir
    .hint = Telegram Desktop no reproduce animaciones más grandes
inspect-features = Características
    .hint = Características de Lottie más allá de formas y transformaciones


## What went wrong, as converting or reading a file reports it

crashed = tgradish falló: { $details }
ffmpeg-not-found = No se encontró ffmpeg: { $details }
ffmpeg-not-on-path = ffmpeg y ffprobe no están en el PATH: instalarlos, o elegir otro ffmpeg en «{ settings }»
ffmpeg-args-need-program = Los argumentos de ffmpeg necesitan el ffmpeg del sistema, que se puede elegir en «{ settings }»; las opciones del codificador funcionan con cualquiera
program-failed = { $program } falló ({ $status }):
unreadable-file = No se pudo leer { $name }: { $details }
no-video = { $name } no contiene video
no-such-file = { $name } ya no está
not-webm = { $name } no es un archivo WebM
not-sticker = { $name } no es un sticker (un archivo WebM, WebP o .tgs)
no-frame-at = { $name } no tiene ningún fotograma en { $seconds }
nothing-fits = Nada cabe en el límite de { $limit }: la codificación más pequeña ocupó { $smallest }. Puede ayudar una parte usada más corta, una frecuencia de fotogramas más baja o un rango de búsqueda más amplio
clipboard-failed = No se pudo pegar del portapapeles: { $details }
output-is-input = El resultado sobrescribiría { $name }, a partir del cual se crea
nothing-to-convert = No hay imágenes que convertir


## Saving and managing presets

save-preset-title = { -save-settings }
save-preset-note = Los ajustes de este archivo, para que cualquier archivo parta de ellos, salvo los que solo le sirven a él: sticker o emoji, recorte, parte usada, etiqueta, capas y hoja de sprites
preset-name = Nombre
    .placeholder = mis stickers
preset-description = Descripción
    .placeholder = opcional
    .none = sin descripción
preset-exists = Ya existe un preajuste con este nombre: al guardar, se reemplaza
replace = Reemplazar
name-it-first = Falta ponerle nombre
cancel = Cancelar
preset-name-length = El nombre de un preajuste debe tener de 1 a { $most } caracteres y no empezar por un espacio
preset-name-characters = El nombre de un preajuste solo puede tener letras, dígitos, espacios, - y _
preset-name-builtin = «{ $name }» es el nombre de un preajuste integrado
preset-name-taken = Otro preajuste se llama «{ $name }»
preset-extended = «{ $other }» se basa en él: cambiar primero ese preajuste
your-presets = Mis preajustes
your-presets-note = Los cambios se guardan al hacerlos. «{ preset-fast }», «{ preset-balanced }» y «{ preset-best }» son integrados y no cambian
no-presets = Aún no hay ninguno: «{ -save-settings }», en la barra bajo los ajustes
preset-broken = Su archivo no se puede leer
delete-it = Eliminarlo
deleted = Se eliminó «{ $name }»
undo = Deshacer
    .hint = Recuperarlo
not-put-back = No se pudo recuperar el preajuste «{ $name }»


## About and licences

about-title = Acerca de y licencias
about-summary = Stickers y emojis de Telegram a partir de videos, imágenes o pixel art. Licencia MIT;
source-code = código fuente
send-feedback = Enviar comentarios por Telegram
about-notices = tgradish también está hecho con el trabajo de otras personas. Sus licencias piden que estos avisos acompañen a cada copia, así que el programa los incluye:
notice-licence = { $name }: licencia
notice-named-licence = { $name }: { $licence }
notice-licence-notes = { $name }: notas de la licencia
notice-patent-grant = { $name }: concesión de patentes
notice-sources = { $name }: qué se incluye y su código fuente
notice-font = { $subsets ->
    [1] Fuente: { $name }, un subconjunto
   *[other] Fuente: { $name }, subconjuntos
}
notice-crates = Crates de Rust
notice-crates-licence = { $licence }: { $crates }
    .more = { $licence }: { $crates } y { $more } más
ffmpeg-sources = El código fuente exacto del ffmpeg integrado, con esta versión


## The dialog of keyboard shortcuts, opened from the menu or by ? or F1

key-ctrl = Ctrl
key-cmd = Cmd
key-alt = Alt
key-shift = Mayús
key-enter = Intro
key-space = Espacio
key-delete = Supr
key-backspace = Retroceso
key-home = Inicio
key-end = Fin
key-escape = Esc
key-left = Izquierda
key-right = Derecha
key-up = Arriba
key-down = Abajo
shortcuts-files = Archivos
shortcuts-playing = Vista previa y línea de tiempo
shortcuts-general = General
shortcuts-mouse = Mouse, en la vista previa y la línea de tiempo
shortcut-add-files = Añadir archivos
shortcut-paste = Añadir los archivos o la imagen del portapapeles
shortcut-select = Seleccionar el archivo de arriba o de abajo
shortcut-convert = Convertir el archivo seleccionado
shortcut-convert-all = Convertir todos: cada archivo sin un resultado actualizado
shortcut-keep-best = Conservar la mejor codificación hasta ahora, mientras se ajusta un WebM
shortcut-copy-result = Copiar el resultado del archivo seleccionado, para pegarlo en Telegram
shortcut-remove = Quitar el archivo seleccionado, o aquel cuya fila tiene el foco
shortcut-undo = Deshacer un cambio en el archivo seleccionado, o recuperar archivos quitados
shortcut-redo = Rehacer un cambio deshecho
shortcut-play = Reproducir o pausar
shortcut-step = Un fotograma atrás o adelante; con Mayús, diez
shortcut-ends = El primer o el último fotograma usado
shortcut-cut-here = Empezar o terminar la parte usada en el fotograma mostrado
shortcut-move-start = Mover el inicio de la parte usada; con Mayús, de diez en diez fotogramas
shortcut-move-end = Mover el final de la parte usada; con Mayús, de diez en diez fotogramas
shortcut-settings = Abrir Ajustes
shortcut-inspect = Inspeccionar un sticker
shortcut-scale-up = Agrandar todo
shortcut-scale-down = Reducir todo
shortcut-scale-reset = Volver al 100%
shortcut-shortcuts = Esta lista
shortcut-close = Cerrar un diálogo o un menú
with-keys = { $hint } ({ $keys })
mouse-drag = Arrastrar
    .does = Recortar, o mover el recorte o sus bordes
mouse-shape = Mayús+arrastrar una esquina
    .does = Mantener la forma del recorte, salvo que haya una relación de aspecto fijada
mouse-centred = Alt+arrastrar un borde
    .does = Redimensionar el recorte desde su centro
mouse-free = Ctrl+arrastrar
    .does = Recortar sin ajustarse a los bordes ni al centro de la imagen
mouse-cancel = Esc al arrastrar
    .does = Devolver el recorte a como estaba
mouse-crop-keys = Clic en el recorte y flechas
    .does = Moverlo un píxel, diez con Mayús; Ctrl+flechas lo redimensiona
mouse-double-click = Doble clic
    .does = Volver a la imagen completa
mouse-wheel = Rueda
    .does = Acercar o alejar
mouse-pan = Arrastrar con el botón derecho o central
    .does = Desplazar la vista ampliada
mouse-pan-across = Mayús+rueda
    .does = Desplazar en horizontal la vista ampliada
mouse-slide = Mayús+arrastrar un corchete
    .does = Deslizar la parte usada de la línea de tiempo, manteniendo su duración
