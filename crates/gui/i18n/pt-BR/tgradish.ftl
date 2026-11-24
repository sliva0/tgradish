### tgradish's words in Brazilian Portuguese. What each message means and
### where it is shown is said in the comments of the English file,
### en-GB/tgradish.ftl.

language-name = Português (Brasil)


## Shared by many places

close = Fechar
back-to-preset = Voltar ao valor da predefinição
custom = Manual
    .hint = Escolha “{ custom }” para definir
webm-sticker = Figurinha WebM
webm-emoji = Emoji WebM
tgs-sticker = Figurinha TGS
tgs-emoji = Emoji TGS
image-sticker = Figurinha estática
image-emoji = Emoji estático
count-colours = { $count ->
    [one] { $count } cor
    [many] { $count } cores
   *[other] { $count } cores
}
count-frames = { $count ->
    [one] { $count } quadro
    [many] { $count } quadros
   *[other] { $count } quadros
}
count-layers = { $count ->
    [one] { $count } camada
    [many] { $count } camadas
   *[other] { $count } camadas
}
count-rectangles = { $count ->
    [one] { $count } retângulo
    [many] { $count } retângulos
   *[other] { $count } retângulos
}
count-tracks = { $count ->
    [one] { $count } faixa
    [many] { $count } faixas
   *[other] { $count } faixas
}
of-limit = { $used } de { $limit }
telegram-accepts = O Telegram deve aceitar o arquivo


## Messages along the bottom

dismiss = Dispensar
    .all = Dispensar todas
said-times = ×{ $count }
details = Detalhes
settings-unreadable = Não foi possível ler o arquivo de configurações; usando as configurações padrão
setting-unsaved = Não foi possível salvar sua alteração no arquivo de configurações
    .unreadable = Sua alteração não foi salva: não foi possível ler o arquivo de configurações
settings-file-note = Ele pode ter sido editado à mão ou por outra versão do tgradish. As alterações não podem ser salvas até que ele seja corrigido ou substituído; nada do que há nele se perde.
fresh-settings = Começar com configurações novas
    .hint = Gravar as configurações em uso como um novo arquivo de configurações; o antigo é mantido ao lado
    .saved = As configurações novas foram salvas
    .kept = O arquivo antigo foi mantido ao lado, como { $name }
    .failed = Não foi possível gravar as configurações novas
read-again = Tentar novamente
    .hint = Ler de novo, depois de corrigido
    .failed = Ainda não foi possível ler
settings-read = O arquivo de configurações foi lido de novo, e suas configurações estão em uso
copy-path = Copiar caminho
    .hint = Copiar o caminho do arquivo, para abri-lo em um editor
    .done = Copiado
files-removed = { $count ->
    [one] { $count } arquivo removido
    [many] { $count } arquivos removidos
   *[other] { $count } arquivos removidos
}
undo-removing = Desfazer
    .hint = Devolvê-los ao lugar onde estavam, com suas configurações e resultados (Ctrl+Z)
settings-reset = As configurações voltaram ao padrão
undo-reset = Desfazer
    .hint = Restaurar as configurações como estavam antes
folder-of = Mostrar na pasta
    .hint = Abrir a pasta onde ele está
presets-unreadable = Não foi possível ler suas predefinições, então só as embutidas estão disponíveis
files-gone = { $count ->
    [one] { $count } arquivo da sessão anterior não existe mais
    [many] { $count } arquivos da sessão anterior não existem mais
   *[other] { $count } arquivos da sessão anterior não existem mais
}
session-unreadable = Não foi possível ler os arquivos da sessão anterior, então nenhum foi aberto; a lista deles foi guardada à parte
files-unreadable = { $count ->
    [one] { $count } arquivo da sessão anterior não pôde ser lido e ficou de fora; a lista como estava foi guardada à parte
    [many] { $count } arquivos da sessão anterior não puderam ser lidos e ficaram de fora; a lista como estava foi guardada à parte
   *[other] { $count } arquivos da sessão anterior não puderam ser lidos e ficaram de fora; a lista como estava foi guardada à parte
}
session-unsaved = Não foi possível guardar os arquivos abertos para a próxima sessão
nothing-to-paste = Nada para colar
    .why = A área de transferência não tem arquivos, caminhos de arquivo nem imagem
paste-failed = Não foi possível ler a área de transferência
    .image = Não foi possível salvar a imagem colada
pick-failed = O seletor de arquivos falhou, então nada foi escolhido


## Dropping files

drop-to-add = Solte para adicionar arquivos


## The system's file dialogs: their titles, which say what each is for,
## and the names of the kinds of files they show

dialog-add-files = Adicionar arquivos para converter
dialog-add-folder = Adicionar uma pasta de quadros
dialog-add-images = Adicionar imagens como uma animação
dialog-inspect = Escolher uma figurinha para inspecionar
dialog-ffmpeg = Escolher o ffmpeg
dialog-results-folder = Escolher onde salvar os resultados
images-filter = Imagens
stickers-filter = Figurinhas


## The list of files, on the left

settings = Configurações
light-theme = Mudar para o tema claro
dark-theme = Mudar para o tema escuro
main-menu = Configurações e mais
menu-undo = Desfazer
    .hint = A última alteração nas configurações do arquivo, ou os arquivos recém-removidos
menu-redo = Refazer
    .hint = A última alteração desfeita
menu-settings = Configurações…
inspect = Inspecionar figurinha…
    .hint = Verificar se um arquivo atende aos limites do Telegram
feedback = Enviar feedback
    .hint = Escrever ao autor no Telegram: no app, se estiver instalado, ou no navegador
about-and-licences = Sobre e licenças…
show-input-in-folder = Mostrar na pasta
    .pasted = Uma imagem colada não tem arquivo próprio
shortcuts = Atalhos de teclado
add-files = Adicionar arquivos…
add-more = Mais formas de adicionar arquivos
add-folder = Uma pasta de quadros…
    .hint = Em ordem alfabética, como uma só animação
add-images = Imagens como uma animação…
    .hint = As imagens escolhidas, em ordem alfabética
add-pasted = Da área de transferência
    .hint = Arquivos copiados, caminhos ou uma imagem
convert-all = { $count ->
    [0] Converter tudo
   *[other] Converter tudo ({ $count })
}
    .hint = Converter todos os arquivos sem resultado atualizado
remove-all = Remover tudo
converting-batch = Convertendo { $current } de { $total }
stop-all = Parar tudo
    .hint = Parar a conversão e tirar da fila os arquivos que aguardam
remove = Remover
remove-file = Remover da lista
convert = Converter
    .hint = Criar a figurinha ou o emoji
row-convert-again = Desatualizado: converter de novo
row-next = próximo
row-waiting = na fila, { $ahead } à frente
row-stale = { $size } · desatualizado
row-failed = falhou
row-exists = já existe um arquivo
row-stopped = interrompido
drop-here = Solte aqui vídeos, GIFs, imagens ou arquivos do Aseprite
drop-here-note = Para converter em figurinha ou emoji de vídeo, animado ou estático
paste = Colar
name-pasted = imagem colada
name-frames = { $name } (quadros)
name-and-more = { $name } e mais { $more }
no-font = { $count ->
    [1] Nenhuma fonte para { $codes } foi encontrada, nem entre as do tgradish nem entre as do sistema, por isso ele aparece pelo código. O arquivo em si está em ordem
   *[other] Nenhuma fonte para { $codes } foi encontrada, nem entre as do tgradish nem entre as do sistema, por isso eles aparecem pelos códigos. O arquivo em si está em ordem
}


## The bar under the settings of the file picked

stop = Parar
stopping = Parando…
keep-best = Manter o melhor
    .hint = Parar de tentar mais codificações e salvar como resultado a melhor que coube até agora
    .nothing-fits = Nada coube ainda: não há codificação para manter
keeping-best = Mantendo o melhor…
cancel-waiting = Cancelar
next-to-convert = Próximo a converter
waiting-for = Na fila, { $ahead } à frente
convert-again = Converter de novo
    .changed = Desatualizado: criar de novo com as configurações atuais
    .hint = Criar de novo com as mesmas configurações
-save-settings = Salvar configurações como predefinição
save-preset = { -save-settings }…
    .hint = Guardar como uma predefinição sua, para qualquer arquivo partir dela
failed = Falhou: { $error }
    .short = Falhou — veja os detalhes
in-the-way = Já existe um arquivo que não é do tgradish:
replace-it = Substituir
stopped = Interrompido
show-in-folder = Mostrar na pasta
saves-as = Salva como
    .hint = O local dos resultados é definido em “{ settings }”
result-saved-as = Salvo como
copy-result = Copiar
    .hint = Copiar a figurinha como arquivo, para colar em uma conversa no Telegram
    .done = Copiado
    .failed = Não copiado
replaces-earlier = Substitui um resultado anterior
    .any = Substitui o arquivo existente


## Reading a file to show it

reading-crashed = O tgradish falhou ao ler isto
reading-stopped = A leitura foi interrompida
no-frames = O ffmpeg não encontrou quadros nele
no-frames-there = O ffmpeg não encontrou quadros ali
no-frame-there = O ffmpeg não encontrou nenhum quadro ali
art-no-frames = Não tem quadros
result-no-frames = O resultado não tem quadros


## The result's small picture over the preview

inset-label = Resultado em { $scale }
    .plain = Resultado
    .drag = Arraste para outro canto
inset-scale = { $percent }%
    .times = { $times }×
inset-smaller = Menor
    .same = Não ficaria menor
inset-larger = Maior
    .same = Não ficaria maior


## Over the preview

show-input = Entrada
show-result = Resultado
    .outdated = Resultado ⚠
    .hint = Desatualizado: o resultado foi feito com configurações anteriores
convert-first = Converta primeiro
tgs-unseen = Esta figurinha TGS tem formas que o tgradish não sabe desenhar
result-preview = Prévia do resultado
    .hint = Mostrar / ocultar a prévia pequena ao lado da entrada
    .input-only = Aparece ao lado da entrada, não do resultado
zoom-out = Diminuir zoom
    .hint = Mostrar tudo
    .all = O zoom já está no mínimo
input-hint = Arraste para recortar · role para dar zoom · botão direito para mover
crop-hint = Arraste o recorte ou suas bordas · clique duplo para a imagem toda
result-hint = Role para dar zoom · botão direito para mover
unreadable = Não foi possível ler: { $error }
reading = Lendo…
preview-divider = Linha abaixo da prévia


## The timeline under the preview

play = Reproduzir
    .pause = Pausar
play-keys =
    Espaço — reproduzir ou pausar
    ← / → — um quadro para trás ou para frente
    Shift+← / → — dez quadros para trás ou para frente
    Home / End — o primeiro ou o último quadro usado
    {"["} / ] — iniciar ou terminar o trecho usado no quadro mostrado
    Ctrl+← / → — mover o início do trecho usado
    Alt+← / → — mover o fim do trecho usado
nothing-to-play = Só um quadro é usado: não há o que reproduzir
in-seconds = Segundos
    .hint = Tempos em segundos
in-frames = Quadros
    .hint = Tempos como números de quadro, a partir de 1
    .unknown = Não se sabe onde ficam os quadros da entrada
part-start = Onde o trecho usado começa
    .frame = Onde o trecho usado começa: seu primeiro quadro
part-end = Onde o trecho usado termina
    .frame = Onde o trecho usado termina: seu último quadro
frame-number = quadro { $number }
limit-mark = Limite de 3 segundos do Telegram: o trecho usado além dele é cortado
    .spoofed = Limite de 3 segundos do Telegram: o trecho usado além dele também é reproduzido, pois o Telegram é informado de que a figurinha é mais curta
    .sped-up = Limite de 3 segundos do Telegram: um trecho usado mais longo é reproduzido mais rápido, para caber nele
frame-before = Quadro anterior (←)
    .first = Este é o primeiro quadro
frame-after = Próximo quadro (→)
    .last = Este é o último quadro
frame-picked = O quadro


## What to make, beside the preview

make = Criar
format-video = Vídeo
    .hint = Uma figurinha ou emoji de vídeo: VP9 em WebM, a partir de qualquer vídeo ou imagem
format-animation = Animado
    .hint = Uma figurinha ou emoji animado: arquivo TGS. Esta opção é feita para pixel art: TGS é um formato vetorial, então fica nítido em qualquer tamanho. Ideal para emojis personalizados, que têm só 100 × 100 px. Não serve para vídeos ou fotos comuns: para eles, use “{ format-video }”
format-image = Estático
    .hint = Uma figurinha ou emoji estático: WebP, a partir de qualquer imagem ou de um quadro de vídeo
target-sticker = Figurinha
target-emoji = Emoji personalizado
needs-webm-sticker = WebM, lado maior de 512 px, até 256 KiB
needs-webm-emoji = WebM, 100 × 100 px, até 64 KiB
needs-tgs = TGS, quadrado, até 64 KiB
needs-image-sticker = WebP, lado maior de 512 px, até 512 KiB
needs-image-emoji = WebP, 100 × 100 px, até 512 KiB
not-pixel-art = “{ format-animation }” é feito para pixel art, convertida em vetor para ficar nítida. Isto não parece pixel art: “{ format-video }” combina melhor
preset = Predefinição
    .hint = Quanto esforço dedicar ao resultado; seus próprios valores abaixo têm prioridade
    .image = Uma figurinha estática usa só as configurações de imagem da predefinição, como ela fica quadrada e é redimensionada: é salva sem perdas quando cabe, senão na melhor qualidade que ainda caiba
    .broken = O arquivo desta predefinição está corrompido
    .missing = A predefinição “{ $name }” não existe mais: escolha outra
manage-presets = Gerenciar…
    .hint = Renomear, descrever ou excluir suas predefinições
preset-fast = Rápida
    .webm = Ajusta só a taxa de bits, com o codificador rápido: leva segundos, mas fica mais borrado
    .tgs = Leva menos de um segundo, arquivo um pouco maior
preset-balanced = Equilibrada
    .webm = Ajusta a taxa de bits e tenta taxas de quadros menores quando podem ficar melhores
    .tgs = Entre a rápida e a melhor
preset-best = Melhor
    .webm = Como a equilibrada, com o codificador mais lento: leva minutos em vídeos longos
    .tgs = Leva alguns segundos, o menor arquivo
preset-described = { $name }: { $description }


## Settings: the picture

picture = Imagem
picture-size = Tamanho
once-read = Aparece quando a entrada for lida
crop = Recorte
    .hint = A parte da entrada a usar, em pixels. Arraste na prévia para alterar
margin-top = Margem superior: pixels cortados de cima
margin-left = Margem esquerda: pixels cortados da esquerda
margin-right = Margem direita: pixels cortados da direita
margin-bottom = Margem inferior: pixels cortados de baixo
width = Largura
height = Altura
crop-keys = Recorte: as setas o movem, Ctrl+setas o redimensionam; com Shift, de dez em dez pixels
whole-picture = Imagem inteira
    .hint = Usar a imagem toda
    .exact = Usar a imagem toda, ou o quanto a escala fixa pegar a partir do canto superior esquerdo
middle = Centro
    .hint = Usar do centro o quanto a escala fixa pegar
aspect-ratio = Proporção
    .hint = A proporção entre largura e altura que o recorte mantém ao ser arrastado
    .square = TGS precisa ser quadrado, e “{ square }” está em “{ square-off }”, que mantém a forma do recorte: escolher isto o deixa quadrado de novo
shape-free = Livre
    .hint = Qualquer forma
shape-preserve = Original
    .hint = A forma da própria entrada
shape-custom = Personalizada
    .hint = Uma forma digitada: como proporção ou como o tamanho do resultado
    .pick = Escolha “{ shape-custom }” para digitar uma forma
ratio-result = Resultado
    .fixed = O resultado é forçado a quadrado ou tem margens, então seu tamanho não segue o recorte
    .width = Largura do resultado
    .height = Altura do resultado
square = Forçar quadrado
    .hint = Como o resultado fica quadrado
    .tgs = Como a pixel art fica quadrada (como todo TGS precisa ser)
square-off = Desativado
    .hint = O resultado mantém a forma do recorte
    .emoji = Emojis precisam ser quadrados
    .tgs = TGS precisa ser quadrado, e o recorte que ele mantém também: escolher isto define “{ aspect-ratio }” como 1:1
    .kept = A pixel art mantém a forma do recorte, que “{ aspect-ratio }” mantém quadrada (como todo TGS precisa ser)
square-pad = Preenchendo
    .hint = Margens transparentes preenchem o resto do quadrado
    .tgs = Margens transparentes preenchem o resto do quadrado
square-crop = Recortando
    .hint = O centro preenche o quadrado e o resto é cortado: a prévia o escurece
    .tgs = O centro preenche o quadrado e o resto é cortado
square-stretch = Esticando
    .hint = A imagem é esticada até ficar quadrada
    .tgs = A pixel art é esticada até ficar quadrada (seus pixels deixam de ser quadrados)
square-exact = A escala inteira fixa mantém a imagem toda: use o preenchimento
scaling = Redimensionamento
    .hint = Como os pixels da entrada viram os do resultado
scaling-auto = Automático
    .hint = Pixels nítidos para pixel art ampliada duas vezes ou mais, suave nos outros casos
scaling-smooth = Suave
    .hint = Pixels vizinhos se misturam
scaling-sharp = Pixels nítidos
    .hint = Cada pixel vira um bloco, e os blocos só se misturam nas bordas
scaling-integer = Inteiro
    .hint = Cada pixel vira o mesmo número inteiro de pixels do resultado, ou blocos inteiros deles viram um só; margens transparentes preenchem o resto
integer-scale = Escala inteira
    .hint = Quantos pixels do resultado cada pixel da entrada vira, ou quantos pixels da entrada formam um
    .only = Só com o redimensionamento “{ scaling-integer }”
scale-largest = Maior que caiba
    .hint = O maior múltiplo inteiro que mantém o recorte dentro do resultado
    .preset = A predefinição fixa uma escala
scale-fixed = Fixa
    .hint = Esta escala; o recorte pode ser no máximo o que ela faz caber no resultado
smaller = Menor
larger = Maior
scale-field = Digite 3 para três vezes maior, ou 1/3 para um terço
    .pick = Escolha “{ scale-fixed }” para definir uma escala
scale-unused = Escalas inteiras só valem com o redimensionamento “{ scaling-integer }”
scale-unsuited = O recorte ainda não combina com esta escala
scale-makes = { $scale } deixa o recorte com { $width } × { $height } px
scale-makes-padded = { $scale } deixa o recorte com { $width } × { $height } px; margens transparentes preenchem o resto dos { $box_width } × { $box_height }
scale-each-way = ×{ $across } na horizontal e ×{ $down } na vertical
scale-shrinks = O recorte é maior que o resultado, então é reduzido com pixels nítidos: uma escala fixa como ×1/2 o reduz em blocos inteiros
margins = Margens
    .hint = Linhas e colunas das bordas que são transparentes em todos os quadros da entrada
auto-crop = Recortar margens vazias automaticamente
    .hint = Corta a pixel art no menor retângulo com pixels visíveis em algum quadro
    .after = Aplicado depois do recorte acima
art-pixel = Pixel da arte
    .hint = Quantos pixels da entrada um pixel da arte ocupa. Pixels fora da grade são movidos para ela
art-pixel-detected = Detectado
    .hint = Encontrado na pixel art: seus menores blocos de uma só cor


## Settings: time

time = Tempo
used-part = Trecho usado
    .hint = Onde começa e termina o trecho da entrada usado; a linha do tempo sob a prévia também o define
part-from = De
part-to = até
part-first = Primeiro quadro
part-last = Último quadro
all-of-input = Toda a entrada
part-seconds = { $from } a { $to } de { $length }
part-frame = Quadro { $number } de { $count }
part-frames = Quadros { $first } a { $last } de { $count }
speed = Velocidade
    .hint = Quantas vezes mais rápido que a entrada o resultado é reproduzido
from-preset = Da predefinição
    .hint = O valor que a predefinição define
from-input = Da entrada
    .speed = Na velocidade da entrada
    .rate = A taxa da entrada nessa velocidade, no máximo 60 fps
fits-3-s = Cabe em 3 s
    .hint = Rápido o bastante para o trecho usado caber em 3 segundos
fit-3-s = Ajustar a 3 s
    .hint = Definir a velocidade para o trecho usado durar 3 segundos
plays = Dura { $played }
plays-cut = Dura { $kept } dos { $used }: o resto é cortado
plays-spoofed = Dura todos os { $played }: o Telegram é informado de que dura menos
frame-rate = Taxa de quadros
    .hint = Menos quadros por segundo deixam mais bytes para cada um; mais que os da entrada só os repetiriam
    .sequence = A velocidade com que os quadros da sequência são reproduzidos
    .sheet = A velocidade com que os quadros de uma sequência ou folha de sprites são reproduzidos
default-rate = Padrão
    .hint = { $fps } fps, pois estes quadros não dizem a que velocidade são reproduzidos
over-3-s = Falsificar duração
    .hint = O Telegram recusa figurinhas que dizem durar mais de 3 segundos
spoof-auto = Quando mais longa
    .hint = Figurinhas com mais de 3 segundos recebem uma duração curta gravada nelas: o Telegram as aceita e reproduz tudo
spoof-always = Sempre
    .hint = Até figurinhas curtas recebem a duração curta gravada nelas
spoof-never = Nunca: cortar em 3 s
    .hint = Não falsifica os metadados e corta o vídeo em 3 segundos
spoof-to = Duração falsificada
    .hint = A duração gravada nos metadados, que o Telegram lê
    .honest = Nada é falsificado com “{ spoof-never }” escolhido
default-duration = Padrão
    .hint = Uma duração curta que o Telegram aceita
frame = Quadro
frame-at = Quadro em
    .hint = O momento do quadro do qual a imagem é feita; a linha do tempo sob a prévia também o escolhe


## Settings: size and quality of a WebM

size-and-quality = Tamanho e qualidade
fitting = Ajuste ao limite
    .hint = O que o tgradish altera para ficar logo abaixo do limite de tamanho do arquivo
    .lossless = A codificação sem perdas não tem taxa de bits nem qualidade a ajustar: escolher isto desmarca “{ lossless-encoding }”
fit-auto = Melhor aparência
    .hint = Ajusta a taxa de bits e, quando nada cabe ou quando menos quadros, cada um com mais bits, podem ficar melhores, tenta taxas de quadros menores e fica com o que mais se parece com a entrada
fit-bitrate = Taxa de bits
    .hint = Ajusta a taxa de bits na taxa de quadros escolhida
fit-crf = Qualidade
    .hint = Ajusta o valor de qualidade constante (CRF)
fit-fps = Taxa de quadros
    .hint = Ajusta a taxa de quadros com qualidade constante
fit-length = Duração
    .hint = Ajusta a duração com qualidade constante, cortando o fim
fit-off = Desativado
    .hint = Codifica uma vez com os valores abaixo, mesmo que fique grande demais
encoder = Esforço do codificador
    .hint = Uma codificação mais rápida fica pior com o mesmo tamanho
encoder-fast = Rápido
    .hint = Leva segundos, mas fica visivelmente pior com o mesmo tamanho
encoder-balanced = Equilibrado
encoder-best = Melhor
    .hint = Lento: leva minutos em vídeos longos
fitting-off = “{ fit-off }” codifica uma vez
attempts = Codificar até
    .hint = Quantas vezes o ajuste pode codificar, no máximo
attempts-default = Padrão
    .hint = O bastante para ficar só alguns por cento abaixo do limite de tamanho do arquivo na maioria das entradas
attempts-times = { $count ->
    [one] vez
    [many] vezes
   *[other] vezes
}
search-within = Buscar entre
    .hint = O intervalo em que o ajuste busca, na unidade do valor ajustado
    .low = Mínimo
    .high = Máximo
search-auto = Automático
    .hint = Tão amplo quanto convém ao modo de ajuste e à entrada
range-to = e
crf = Qualidade (CRF)
    .hint = Qualidade constante de 0, a melhor, a 63
    .lossless = A codificação sem perdas não usa qualidade
    .fitted = O ajuste encontra a qualidade
    .bitrate = O ajuste encontra uma taxa de bits, não uma qualidade
crf-default = Padrão
    .hint = Uma boa qualidade no tamanho das figurinhas
lossless = Sem perdas
lossless-encoding = Codificação sem perdas
    .hint = Só fica pequena com imagens minúsculas ou paradas
    .switches = Só fica pequena com imagens minúsculas ou paradas. A codificação sem perdas não tem taxa de bits nem qualidade a ajustar: marcar isto muda o ajuste para “{ fit-fps }”
bitrate = Taxa de bits
    .hint = Usada a menos que só uma qualidade seja definida
    .fitted = O ajuste encontra a taxa de bits
    .constant = Codificado com qualidade constante, não com taxa de bits
bitrate-auto = Pelo limite de tamanho
    .hint = O quanto a duração permite caber


## Settings: size and quality of a TGS animation

tgs-encoder = Esforço do codificador
    .hint = Quanto se esforçar para achar uma codificação pequena
tgs-fast = Rápido
    .hint = Leva menos de um segundo, arquivo um pouco maior
tgs-balanced = Equilibrado
tgs-best = Melhor
    .hint = Leva alguns segundos, o menor arquivo
too-large = Se não couber
    .hint = O que acontece quando a pixel art não cabe em 64 KiB como está
never-change = Nunca alterar a pixel art; avisar em vez disso
    .set = A pixel art nunca é alterada, como definido acima
give-up = Sacrificar primeiro
    .hint = Quando a pixel art precisa mudar para caber: seu movimento ou seus detalhes
give-up-either = Qualquer um
    .hint = O que menos altera a pixel art pelos bytes que economiza
give-up-motion = Movimento
    .hint = Quadros se fundem e são descartados primeiro: detalhes completos, movimento menos fluido
give-up-detail = Detalhes
    .hint = A imagem fica mais grosseira primeiro: movimento fluido, menos detalhes
ways-to-fit = Como fazer caber
    .hint = O que o ajuste pode fazer com a pixel art para ela caber em 64 KiB, do menos ao mais visível
reduce-snap = Alinhar à grade de pixels
    .hint = Move pixels soltos para a grade de pixels da arte
reduce-colours = Fundir cores parecidas
    .hint = Cores quase iguais viram uma só
reduce-specks = Remover pontos soltos
    .hint = Pixels isolados que diferem de todos ao redor
reduce-frames = Fundir quadros parecidos
    .hint = Quadros quase iguais mostram uma só imagem
reduce-drop = Descartar quadros
    .hint = Mostra um quadro sim, outro não, ou menos
reduce-resolution = Reduzir a resolução
    .hint = Menos pixels da arte, e maiores
reading-art = Leitura da pixel art
sprite-sheet = Folha de sprites
    .hint = Quadros dispostos em uma grade de células iguais, linha por linha
read-as-sheet = Ler como folha de sprites
sheet-refused = Este arquivo já tem { $frames } quadros: uma folha de sprites é uma só imagem
    .converted = Lido como está, não como folha de sprites: ele já tem { $frames } quadros
sheet-cells = Células
    .columns = Colunas
    .rows = Linhas
sheet-frames = Quadros
sheet-all = Todos
    .hint = Todas as células, exceto as transparentes no fim


## Settings: an Aseprite file's tags and layers

aseprite-file = Arquivo do Aseprite
unreadable-choices = Não dá para ler com a tag e as camadas escolhidas
read-all = Ler tudo
tag = Tag
    .hint = Escolhe os quadros de uma tag: de onde ela começa até onde termina, ou lidos na ordem dela quando ela toca ao contrário ou em ida e volta
all-frames = Todos os quadros
    .hint = Todos os quadros do arquivo
tag-frame = Quadro { $number }
tag-frames = Quadros { $from } a { $to }
tag-reverse = { $frames }, ao contrário
tag-ping-pong = { $frames }, ida e volta
tag-pong-ping = { $frames }, volta e ida
layers = Camadas
    .hint = Quais camadas aparecem, a de cima primeiro; um grupo oculto oculta as camadas dentro dele
    .group-hidden = O grupo dela está oculto
    .file = Voltar às camadas que o arquivo mostra


## Settings: encoder options, folded away under the rest

encoder-options = Opções do codificador
vp9-options = Opções do codificador por nome, como tune-content = screen para gráficos chapados. ffmpeg -h encoder=libvpx-vp9 lista todas
    .name = Por exemplo: tune-content
    .value = Por exemplo: screen
add-option = Adicionar opção
examples = Exemplos:
example-tune-content = Gráficos chapados e gravações de tela: bordas mais nítidas
example-aq-mode = Gasta bits onde a imagem é complexa
example-sharpness = Mantém mais detalhes finos, à custa de alguma suavidade
example-arnr-strength = Menos redução de ruído entre quadros
example-g = Um quadro-chave pelo menos a cada 60 quadros
example-qmax = Nunca pior que a qualidade 50 em nenhum ponto
ffmpeg-arguments = Argumentos do ffmpeg
    .hint = Argumentos brutos adicionados antes da saída, separados como um shell faria
    .placeholder = Por exemplo: -qmin 4
    .unfinished = Uma aspa ou uma barra invertida ficou aberta: os argumentos continuam como foram lidos por último até ela ser fechada
    .system = Só com o ffmpeg do sistema, que pode ser escolhido em “{ settings }”


## What keeps a file from being converted as it is, and the ways out

blocked = Ainda não dá para converter: { $reason }
    .heading = Ainda não dá para converter
blocked-crop = o recorte está fora da imagem
blocked-sheet = as células da folha de sprites não dividem a imagem
blocked-tag = o arquivo não tem a tag “{ $tag }”
blocked-layers = { $count ->
    [1] o arquivo não tem a camada { $layers }
   *[other] o arquivo não tem as camadas { $layers }
}
blocked-ffmpeg = o ffmpeg não foi encontrado
crop-outside = O recorte, { $width } × { $height } a partir de { $left }, { $top }, está fora desta imagem de { $across } × { $down }
use-whole = Usar a imagem inteira
    .hint = Não recortar nada, mantendo a imagem toda
sheet-uneven = A imagem, de { $width } × { $height }, não se divide em { $columns } × { $rows } células iguais
sheet-nearest = Mais próximo que divide: { $columns } × { $rows }
    .hint = O layout mais próximo deste cujas células dividem a imagem
tag-missing = Este arquivo não tem a tag “{ $tag }”
layers-missing = { $count ->
    [1] Este arquivo não tem a camada { $layers }
   *[other] Este arquivo não tem as camadas { $layers }
}
layers-as-file = Mostrar camadas como no arquivo
    .hint = Mostrar e ocultar as camadas como o arquivo faz
needs-ffmpeg = Isto precisa do ffmpeg, que não foi encontrado
    .hint = Instale o ffmpeg pelos pacotes do seu sistema ou pelo ffmpeg.org, ou escolha onde ele está. Os arquivos são lidos de novo assim que ele for encontrado
choose-ffmpeg = Escolher ffmpeg…
    .hint = Escolher o programa ffmpeg, ou uma pasta com ffmpeg e ffprobe
row-attention = requer atenção
    .hint = Algo impede a conversão: veja as configurações dele
row-needs-ffmpeg = requer ffmpeg
    .hint = O ffmpeg o lê ou cria o resultado dele, e não foi encontrado
need-attention = { $count ->
    [one] { $count } requer atenção
    [many] { $count } requerem atenção
   *[other] { $count } requerem atenção
}


## What converting will do, beside the preview, step by step

plan-video = Processamento do vídeo:
plan-picture = Processamento da imagem:
plan-animation = Processamento da animação:
plan-art = Processamento da pixel art:
plan-video-frame = Processamento de um quadro do vídeo:
plan-animation-frame = Processamento de um quadro da animação:
plan-art-frame = Processamento de um quadro da pixel art:
plan-tag = Reprodução da tag “{ $tag }”
plan-tag-reverse = Reprodução da tag “{ $tag }”, ao contrário
plan-tag-ping-pong = Reprodução da tag “{ $tag }”, em ida e volta
plan-tag-pong-ping = Reprodução da tag “{ $tag }”, em volta e ida
plan-cropped = Recorte para { $width } × { $height }
plan-frame = Uso do quadro em { $time }
plan-scaled = Redimensionamento para { $width } × { $height }
plan-stretched-across = Esticamento ×{ $factor } na horizontal
plan-stretched-down = Esticamento ×{ $factor } na vertical
plan-middle-cut = Corte do centro para { $width } × { $height }
plan-padded = Preenchimento até { $width } × { $height }
plan-trimmed = Corte das bordas até os pixels visíveis
plan-grid = Alinhamento dos pixels a uma grade de { $scale } × { $scale }
plan-square-crop = Recorte em quadrado
plan-square-stretch = Esticamento em quadrado
plan-square-pad = Preenchimento até um quadrado
plan-cut = Corte em { $from }–{ $to } de { $length }
plan-faster = Aceleração ×{ $factor }
plan-slower = Desaceleração ×{ $factor }
plan-cut-3-s = Corte em 3 s de { $played }
plan-spoofed = Duração falsificada, para reproduzir todos os { $played }
plan-spoofed-anyway = Duração falsificada, embora { $played } seja curto o bastante
plan-saved = Gravação em { $name }


## Settings: the same conversion as commands, folded away under the rest

commands = Comandos
copy = Copiar
commands-posix = A mesma conversão na linha de comando, para um shell POSIX:
commands-powershell = A mesma conversão na linha de comando, para o PowerShell:
pasted-temporary = Uma imagem colada fica em uma pasta temporária, que some quando o tgradish fecha
ffmpeg-commands = Os comandos do ffmpeg da primeira codificação; o ajuste muda a taxa de bits ou a qualidade, e talvez a taxa de quadros, entre uma codificação e outra:
    .builtin = O que o ffmpeg embutido faz na primeira codificação, como comandos para o ffmpeg do sistema; o ajuste muda a taxa de bits ou a qualidade, e talvez a taxa de quadros, entre uma codificação e outra:
no-ffmpeg-commands = Sem comandos do ffmpeg: { $why }
    .unread = a entrada ainda não foi lida
    .art = o ffmpeg lê um PNG que cada conversão desenha a partir da pixel art


## The Settings dialog

save = Salvar
saved-to = Salvo em { $path }
    .kept = Salvo em { $path }; o arquivo que não pôde ser lido foi mantido como { $kept }
not-saved = Não foi possível salvar: { $error }
no-home = Não há pasta pessoal para guardar as configurações
settings-note = As alterações são aplicadas e salvas na hora
reset-settings = Restaurar padrões
    .hint = Devolver cada configuração daqui ao valor original do tgradish
ffmpeg-given = Esta execução usa o ffmpeg indicado ao iniciar o tgradish; estas configurações valem quando ele é iniciado sem indicar um
ffmpeg-use = Usar
    .hint = Os vídeos são lidos, e as figurinhas WebM criadas, com o ffmpeg
ffmpeg-auto = Automático
    .hint = O ffmpeg embutido, se esta versão tiver um, senão o do sistema
ffmpeg-builtin = Embutido
    .hint = O ffmpeg dentro do tgradish
    .missing = Esta versão do tgradish não tem ffmpeg embutido
ffmpeg-system = Do sistema
    .hint = ffmpeg e ffprobe no PATH; permite argumentos brutos do ffmpeg
ffmpeg-program = Programa
    .hint = Um executável do ffmpeg, ou uma pasta com ffmpeg e ffprobe; tem prioridade sobre a escolha acima
    .missing = Não há nada ali, então o programa em uso continua o mesmo
ffmpeg-found = { $path }, encontrado no PATH
for-example = Por exemplo: { $example }
choose = Escolher…
check = Verificar
    .hint = Verificar se este ffmpeg consegue criar figurinhas
checking-ffmpeg = Verificando…
ffmpeg-ready = ffmpeg { $version } com VP9: pronto
ffmpeg-no-vp9 = O ffmpeg { $version } não codifica VP9 com libvpx
converting-settings = Conversão
threads = Threads
    .hint = Quantas a conversão usa ao mesmo tempo, no máximo; menos deixam mais do computador para o resto, e demoram mais
threads-auto = Automático
    .hint = Tantas quantas cada parte da conversão aproveitar
results = Resultados
save-results = Salvar
next-to-input = Ao lado de cada entrada
in-a-folder = Em uma pasta
    .unchosen = Até uma pasta ser escolhida, os resultados são salvos ao lado de cada entrada
    .relative = Escreva o caminho completo, desde a raiz, para que a pasta em uso possa mudar
    .file = Isso é um arquivo, não uma pasta, então a pasta em uso continua a mesma
existing-files = Arquivos existentes
    .hint = Resultados criados pelo tgradish sempre são substituídos ao serem criados de novo
replace-in-the-way = Substituir qualquer arquivo que já exista
interface = Interface
language = Idioma
    .hint = O idioma mostrado aqui; a linha de comando continua em inglês
    .variable = Escolhido pela variável de ambiente { $variable }
as-the-system = Igual ao sistema
as-the-system-language = Igual ao sistema: { $language }
theme = Tema
    .hint = O botão ao lado de ⚙ também alterna entre claro e escuro
theme-light = Claro
theme-dark = Escuro
accent = Cor de destaque
    .hint = O que marca o escolhido, o que está sob o mouse e o que tem o foco
scale = Escala
    .hint = O tamanho do texto e dos controles
    .keys = { $larger } e { $smaller } também a alteram; { $reset } volta a 100%
scale-smaller = Menor
scale-larger = Maior
scale-reset = Voltar a 100%
scale-percent = { $percent }%
accent-system = { as-the-system }
    .hint = A cor de destaque da área de trabalho, ou o azul do Breeze quando ela não tem uma
accent-pink = Rosa
accent-red = Vermelho
accent-orange = Laranja
accent-yellow = Amarelo
accent-green = Verde
accent-teal = Verde-azulado
accent-blue = Azul
accent-purple = Roxo
accent-violet = Violeta
accent-grey = Cinza
accent-own = Cor personalizada
    .chosen = Cor personalizada, { $colour }
accent-shade = Tom
    .value = { $saturation }% de saturação, { $value }% de brilho
accent-hue = Matiz
accent-hex = Hex
    .wrong = Escreva como # e seis dígitos hexadecimais, como #3daee9
files = Arquivos
reopen-files = Reabrir os arquivos da sessão anterior, com suas configurações
motion = Movimento
    .hint = Quando o sistema pede menos movimento, nada se move de qualquer forma
motion-on = Suavizar a rolagem, e esmaecer e mover o que muda
new-files = Novos arquivos
preset-for-webm = Predefinição WebM
preset-for-tgs = Predefinição TGS


## The result, beside the preview

last-conversion = Última conversão
path-in-the-way = Já existe { $path }
stopped-early = Interrompido antes de terminar
result = Resultado
    .stale = Resultado desatualizado
result-copy = Copie ({ $keys }) para colar em uma conversa no Telegram
result-webm = { $width } × { $height } px, { $fps } fps, { $length }
    .spoofed = { $width } × { $height } px, { $fps } fps, { $length }, duração falsificada
changed-to-fit = Alterado para caber: veja como abaixo
replaced-earlier = Substituiu um resultado anterior feito pelo tgradish
converting = Convertendo
stage-reading = Lendo a pixel art
stage-drawing = Convertendo para vetor
stage-fitting-steps = { $count ->
    [one] Ajustando para 64 KiB: { $count } etapa, cerca de { $size }
    [many] Ajustando para 64 KiB: { $count } etapas, cerca de { $size }
   *[other] Ajustando para 64 KiB: { $count } etapas, cerca de { $size }
}
stage-fitting-from = Ajustando para 64 KiB a partir de { $size }
stage-compressing = Compactando
    .hopeless = Compactando rápido: não vai caber
    .missed = Compactado → { $size }, grande demais
    .missed-far = Compactando: ainda muito grande
retry = Tentativa { $number }: mirando em { $aim }
retry-done = Tentativa { $number }: mirando em { $aim } → { $size }, grande demais
    .far = Tentativa { $number }: mirando em { $aim } → ainda muito grande
starting = Iniciando
encode = Codificação { $number }: { $fps } fps, { $rate }
encode-of = Codificação { $number } de até { $most }: { $fps } fps, { $rate }
encode-done = Codificação { $number }: { $fps } fps, { $rate } → { $size }, cabe
    .too-large = Codificação { $number }: { $fps } fps, { $rate } → { $size }, grande demais
rate-quality = qualidade { $crf }
rate-lossless = sem perdas
encodes-up-to = { $count ->
    [one] Até { $count } codificação, até uma ficar logo abaixo do limite de tamanho do arquivo
    [many] Até { $count } codificações, até uma ficar logo abaixo do limite de tamanho do arquivo
   *[other] Até { $count } codificações, até uma ficar logo abaixo do limite de tamanho do arquivo
}
art-size = { $width } × { $height } pixels da arte
    .scaled = { $width } × { $height } pixels da arte, cada um com { $scale } × { $scale } pixels da entrada
art-summary = { $size }; { $colours }, { $frames }
art-tempo = Velocidade ×{ $tempo } em relação à entrada
art-off-grid = A maior parte parece pixel art { $scale }× com alguns pixels fora da grade; um tamanho de pixel da arte de { $scale } os alinha
art-shapes = { $layers } com { $rectangles }, { $size } descompactado
art-before-fitting = Cerca de { $size } como está; alterado para caber:
art-step = { $change }, cerca de { $size } depois
step-snapped = pixels alinhados a uma grade de { $scale }
step-colours = cores a menos de { $distance }% de distância fundidas
step-frames = quadros que mudam menos de { $changed }% fundidos
step-dropped = { $share }% dos quadros descartados
step-specks = pixels isolados removidos
step-resolution = resolução reduzida a { $factor }%
column-number = Cada codificação, em ordem; ✔ a mantida
column-fps = Quadros por segundo
column-rate = Taxa de bits, ou a qualidade usada na codificação
column-size = tamanho
    .hint = Fração do limite de tamanho do arquivo, em verde quando cabe
column-alike = similar
    .hint = O quanto ela se parece com a entrada, medido pelo SSIM (similaridade estrutural)
rate-quality-short = q { $crf }
decided-in-band = Parou: a codificação { $number } usa { $share } do limite, pelo menos os { $least } pretendidos
decided-alike-enough = Parou: a codificação { $number } já se parece { $alike } com a entrada, então mais bits só a deixariam maior
decided-alike-unchanged = Parou: a codificação { $number } se parece { $alike } com a entrada, pouco mais que a codificação { $than } com menos bits
decided-little-to-gain = Parou: a codificação { $number } usa { $share } do limite, e outra usaria só cerca de { $predicted }
decided-too-large-at-least = Parou: mesmo o mínimo do intervalo, na codificação { $number }, usa { $share } do limite
decided-nothing-left = Parou: não resta nenhuma configuração que valha a pena tentar
decided-out-of-attempts = { $count ->
    [one] Parou: a única codificação permitida foi feita
    [many] Parou: todas as { $count } codificações permitidas foram feitas
   *[other] Parou: todas as { $count } codificações permitidas foram feitas
}
decided-lowers-to-fit = Tentando { $fps } fps: nada coube a { $from } fps
decided-lowers-for-looks = Tentando { $fps } fps: a codificação { $number } a { $from } fps se parece só { $alike } com a entrada
decided-settled = Parou cedo: codificação { $number } mantida
decided-lower-no-better = Parou: a { $fps } fps, a semelhança com a entrada é { $alike }, não mais que os { $kept_alike } da codificação { $kept } a { $kept_fps } fps
issue-not-webm = O contêiner é { $container }, não WebM
issue-no-video = Não tem faixa de vídeo
issue-not-vp9 = O codec é { $codec }, não VP9
issue-audio = Tem uma faixa de áudio
issue-size = O tamanho é { $width } × { $height } px, mas o Telegram aceita { $side } × { $side } px
    .sticker = O tamanho é { $width } × { $height } px, mas o Telegram aceita um lado de { $side } px e o outro de no máximo isso
issue-too-big = Tem { $size }, acima do limite de { $limit }
issue-too-long = Os metadados dizem que dura { $seconds }, acima do limite de { $limit }
issue-too-many-fps = A taxa de quadros é { $fps } fps, acima do limite de { $limit } fps
issue-animated = É animado, mas figurinhas estáticas são uma só imagem
issue-frame-rate = A taxa de quadros é { $fps } fps, mas o Telegram aceita { $required } fps
    .accepted = A taxa de quadros é { $fps } fps: contra as regras do Telegram, embora o Telegram aceite
issue-no-frames = Não tem quadros
issue-unpacked = Descompactado, tem { $size }, acima do limite de { $limit } do Telegram Desktop
issue-count =
    .layers = Camadas: { $count }, acima do limite de { $limit }
    .shapes-per-layer = Máximo de formas numa camada: { $count }, acima do limite de { $limit }
    .paint-points = Máximo de pontos de caminho num preenchimento: { $count }, acima do limite de { $limit }
    .paints-per-layer = Máximo de preenchimentos e contornos numa camada: { $count }, acima do limite de { $limit } dos apps do Telegram
    .paint-sources = Máximo de formas pintadas numa camada: { $count }, acima do limite de { $limit } dos apps do Telegram
    .path-points = Máximo de pontos num caminho: { $count }, acima do limite de { $limit } dos apps do Telegram
    .keyframes = Máximo de quadros-chave numa propriedade: { $count }, acima do limite de { $limit } dos apps do Telegram
    .assets = Assets: { $count }, acima do limite de { $limit } dos apps do Telegram
    .expanded-layers = Camadas com as pré-composições expandidas: { $count }, acima do limite de { $limit } dos apps do Telegram
    .nesting = Níveis de aninhamento no JSON: { $count }, acima do limite de { $limit } dos apps do Telegram
    .group-depth = Níveis de grupos dentro de grupos: { $count }, acima do limite de { $limit } dos apps do Telegram
issue-cost = Custo de desenho: { $cost }, acima do limite de { $limit }; formas: { $shapes }, camadas: { $layers }, { $each } cada
issue-empty-tangents = Caminhos com tangentes vazias ([]), que o Telegram recusa: { $count }; [0,0] funciona
issue-path-coordinate = Maior coordenada de caminho: { $coordinate }, acima do limite de { $limit } dos apps do Telegram
issue-precomp-loop = As pré-composições se referem umas às outras em ciclo
issue-feature = { $feature }: contra as regras do Telegram, embora o Telegram aceite
    .undrawn = { $feature }: contra as regras do Telegram; o Telegram aceita, mas os apps dele não desenham isso
    .ignored = { $feature }: os apps do Telegram não desenham isso
warned-fewer-frames = Lido a { $fps } fps, pois mais quadros de { $width } × { $height } não caberiam na memória
warned-many-colours = Tem { $colours }, algo incomum para pixel art, então a figurinha ficará grande
warned-length-capped = As durações que o ajuste testa foram limitadas a { $length }, pois ele só encurta
warned-input-shorter = A entrada dura só { $length } depois do início: ela é usada inteira
warned-trimmed = Cortado em { $length }: a falsificação está desligada, e o Telegram recusa vídeos mais longos
warned-left-out = Gravado sem o nome do tgradish nos metadados, pois os argumentos brutos do ffmpeg não deixaram espaço para ele: { $details }
    .duration = Gravado sem a duração falsificada nos metadados, pois os argumentos brutos do ffmpeg não deixaram espaço para ela: { $details }


## Inspecting a sticker

inspect-another = Inspecionar outra…
inspect-webm-sticker = Uma figurinha de vídeo
inspect-webm-emoji = Um emoji de vídeo
inspect-image-sticker = Uma figurinha estática
inspect-image-emoji = Um emoji estático
inspect-tgs = Uma animação vetorial, { $width } × { $height } a { $fps } fps
inspect-unknown = Um tipo de figurinha que esta versão não sabe mostrar
inspect-size = Tamanho
    .hint = O Telegram recusa arquivos maiores
    .tgs = O Telegram recusa figurinhas maiores
inspect-duration = Duração
    .hint = O Telegram lê a duração nos metadados; a falsificação a deixa curta
    .spoofed = { $seconds } s nos metadados, falsificada
    .no-header = { $seconds }, nenhuma nos metadados
inspect-plays = Duração real
    .hint = Quanto o vídeo dura de verdade
inspect-frame-rate = Taxa de quadros
    .value = { $fps } fps de { $most }, { $frames }
inspect-picture = Imagem
    .none = nenhuma
inspect-video = { $width } × { $height } px, { $codec }
    .alpha = { $width } × { $height } px, { $codec }, com transparência
inspect-webp = { $width } × { $height } px, WebP, sem perdas
    .alpha = { $width } × { $height } px, WebP, sem perdas, com transparência
    .lossy = { $width } × { $height } px, WebP, com perdas
    .lossy-alpha = { $width } × { $height } px, WebP, com perdas, com transparência
inspect-audio = Áudio
inspect-title = Título
inspect-made-with = Feito com
    .hint = Os programas que gravaram o arquivo, segundo ele
    .image = O programa que os metadados indicam
    .unknown = desconhecido
inspect-note = Nota
    .hint = Texto no preenchimento do arquivo, que os players ignoram
inspect-name = Nome
    .hint = O nome próprio da animação, que o Telegram não mostra
inspect-length = Duração
    .hint = Figurinhas animadas duram no máximo 3 segundos
    .value = { $seconds } de 3.00 s, { $frames }
inspect-cost = Custo de desenho
    .hint = Formas mais 9 por camada, como o Telegram conta
inspect-layers = Camadas
inspect-shapes = Formas por camada
inspect-refuses-more = O Telegram recusa mais
inspect-points = Pontos por preenchimento
    .hint = Pontos de caminho que um preenchimento pinta; o Telegram recusa mais
inspect-unpacked = Descompactado
    .hint = O Telegram Desktop não reproduz animações maiores
inspect-features = Recursos
    .hint = Recursos do Lottie além de formas e transformações


## What went wrong, as converting or reading a file reports it

crashed = O tgradish falhou: { $details }
ffmpeg-not-found = O ffmpeg não foi encontrado: { $details }
ffmpeg-not-on-path = ffmpeg e ffprobe não estão no PATH: instale-os ou escolha outro ffmpeg em “{ settings }”
ffmpeg-args-need-program = Os argumentos do ffmpeg exigem o ffmpeg do sistema, que pode ser escolhido em “{ settings }”; as opções do codificador funcionam com qualquer um
program-failed = { $program } falhou ({ $status }):
unreadable-file = Não foi possível ler { $name }: { $details }
no-video = { $name } não tem vídeo
no-such-file = { $name } não existe mais
not-webm = { $name } não é um arquivo WebM
not-sticker = { $name } não é uma figurinha (um arquivo WebM, WebP ou .tgs)
no-frame-at = { $name } não tem quadro em { $seconds }
nothing-fits = Nada cabe no limite de { $limit }: a menor codificação teve { $smallest }. Um trecho usado mais curto, uma taxa de quadros menor ou um intervalo de busca mais amplo podem ajudar
clipboard-failed = Não foi possível colar da área de transferência: { $details }
output-is-input = O resultado seria gravado sobre { $name }, a partir do qual ele é feito
nothing-to-convert = Não há imagens para converter


## Saving and managing presets

save-preset-title = { -save-settings }
save-preset-note = As configurações deste arquivo, para qualquer arquivo partir delas, exceto as que só servem para ele: figurinha ou emoji, recorte, trecho usado, tag, camadas e folha de sprites
preset-name = Nome
    .placeholder = minhas figurinhas
preset-description = Descrição
    .placeholder = opcional
    .none = sem descrição
preset-exists = Já existe uma predefinição com este nome: salvar a substitui
replace = Substituir
name-it-first = Dê um nome primeiro
cancel = Cancelar
preset-name-length = O nome de uma predefinição tem de 1 a { $most } caracteres e não começa com espaço
preset-name-characters = O nome de uma predefinição só tem letras, dígitos, espaços, - e _
preset-name-builtin = “{ $name }” é o nome de uma predefinição embutida
preset-name-taken = Outra predefinição já se chama “{ $name }”
preset-extended = “{ $other }” se baseia nela: altere aquela predefinição primeiro
your-presets = Suas predefinições
your-presets-note = As alterações são salvas na hora. “{ preset-fast }”, “{ preset-balanced }” e “{ preset-best }” são embutidas e não mudam
no-presets = Nenhuma ainda: “{ -save-settings }”, na barra abaixo das configurações
preset-broken = Não foi possível ler o arquivo dela
delete-it = Excluir
deleted = “{ $name }” excluída
undo = Desfazer
    .hint = Restaurá-la
not-put-back = Não foi possível restaurar a predefinição “{ $name }”


## About and licences

about-title = Sobre e licenças
about-summary = Figurinhas e emojis do Telegram a partir de vídeos, imagens ou pixel art. Licença MIT;
source-code = código-fonte
send-feedback = Enviar feedback no Telegram
about-notices = O tgradish também é feito do trabalho de outras pessoas. As licenças delas pedem que estes avisos acompanhem cada cópia, por isso o programa os traz:
notice-licence = { $name }: licença
notice-named-licence = { $name }: { $licence }
notice-licence-notes = { $name }: notas da licença
notice-patent-grant = { $name }: concessão de patente
notice-sources = { $name }: o que vem embutido, e seu código-fonte
notice-font = { $subsets ->
    [1] Fonte: { $name }, um subconjunto
   *[other] Fonte: { $name }, subconjuntos
}
notice-crates = Crates do Rust
notice-crates-licence = { $licence }: { $crates }
    .more = { $licence }: { $crates } e mais { $more }
ffmpeg-sources = O código-fonte exato do ffmpeg embutido, com esta versão


## The dialog of keyboard shortcuts, opened from the menu or by ? or F1

key-ctrl = Ctrl
key-cmd = Cmd
key-alt = Alt
key-shift = Shift
key-enter = Enter
key-space = Espaço
key-delete = Delete
key-backspace = Backspace
key-home = Home
key-end = End
key-escape = Esc
key-left = Esquerda
key-right = Direita
key-up = Cima
key-down = Baixo
shortcuts-files = Arquivos
shortcuts-playing = A prévia e sua linha do tempo
shortcuts-general = Geral
shortcuts-mouse = O mouse, na prévia e na linha do tempo
shortcut-add-files = Adicionar arquivos
shortcut-paste = Adicionar os arquivos ou a imagem da área de transferência
shortcut-select = Escolher o arquivo acima ou abaixo
shortcut-convert = Converter o arquivo escolhido
shortcut-convert-all = Converter tudo: todos os arquivos sem resultado atualizado
shortcut-keep-best = Manter a melhor codificação até agora, durante o ajuste de um WebM
shortcut-copy-result = Copiar o resultado do arquivo escolhido, para colar no Telegram
shortcut-remove = Remover o arquivo escolhido, ou aquele cuja linha tem o foco
shortcut-undo = Desfazer uma alteração no arquivo escolhido, ou restaurar arquivos removidos
shortcut-redo = Refazer uma alteração desfeita
shortcut-play = Reproduzir ou pausar
shortcut-step = Um quadro para trás ou para frente; com Shift, dez
shortcut-ends = O primeiro ou o último quadro usado
shortcut-cut-here = Iniciar ou terminar o trecho usado no quadro mostrado
shortcut-move-start = Mover o início do trecho usado; com Shift, de dez em dez quadros
shortcut-move-end = Mover o fim do trecho usado; com Shift, de dez em dez quadros
shortcut-settings = Abrir as configurações
shortcut-inspect = Inspecionar uma figurinha
shortcut-scale-up = Aumentar tudo
shortcut-scale-down = Diminuir tudo
shortcut-scale-reset = Voltar a 100%
shortcut-shortcuts = Esta lista
shortcut-close = Fechar uma janela ou um menu
with-keys = { $hint } ({ $keys })
mouse-drag = Arrastar
    .does = Recortar, ou mover o recorte ou suas bordas
mouse-shape = Shift+arrastar um canto
    .does = Manter a forma do recorte, a menos que uma proporção esteja definida
mouse-centred = Alt+arrastar uma borda
    .does = Redimensionar o recorte em torno do centro
mouse-free = Ctrl+arrastar
    .does = Recortar sem se prender às bordas e ao centro da imagem
mouse-cancel = Esc ao arrastar
    .does = Restaurar o recorte anterior
mouse-crop-keys = Clicar no recorte, depois setas
    .does = Movê-lo um pixel, dez com Shift; Ctrl+setas o redimensionam
mouse-double-click = Clique duplo
    .does = Voltar à imagem inteira
mouse-wheel = Rolar
    .does = Aproximar ou afastar o zoom
mouse-pan = Arrastar com o botão direito ou do meio
    .does = Mover a visualização, com zoom
mouse-pan-across = Shift+rolar
    .does = Mover a visualização na horizontal, com zoom
mouse-slide = Shift+arrastar um colchete
    .does = Deslizar o trecho usado na linha do tempo, mantendo sua duração
