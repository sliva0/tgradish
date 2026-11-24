### tgradish's words in Turkish. What each message means and where it is
### shown is said in the comments of the English file, en-GB/tgradish.ftl.

language-name = Türkçe


## Shared by many places

close = Kapat
back-to-preset = Hazır ayarın değerine dön
custom = Özel
    .hint = Ayarlamak için “{ custom }” seçeneğini seçin
webm-sticker = WebM çıkartma
webm-emoji = WebM emoji
tgs-sticker = TGS çıkartma
tgs-emoji = TGS emoji
image-sticker = Statik çıkartma
image-emoji = Statik emoji
count-colours = { $count } renk
count-frames = { $count } kare
count-layers = { $count } katman
count-rectangles = { $count } dikdörtgen
count-tracks = { $count } iz
percent = %{ $number }
of-limit = { $used } / { $limit }
telegram-accepts = Telegram bunu kabul etmeli


## Messages along the bottom

dismiss = Kapat
    .all = Tümünü kapat
said-times = ×{ $count }
details = Ayrıntılar
settings-unreadable = Ayarlar dosyası okunamıyor, bu yüzden varsayılanlar kullanılıyor
setting-unsaved = Değişiklik ayarlar dosyasına kaydedilemedi
    .unreadable = Değişiklik kaydedilmedi: ayarlar dosyası okunamıyor
settings-file-note = Elle ya da tgradish'in başka bir sürümüyle düzenlenmiş olabilir. Düzeltilene ya da yenisiyle değiştirilene kadar değişiklikler kaydedilemez; içindeki hiçbir şey kaybolmaz.
fresh-settings = Yeni ayarlarla başla
    .hint = Şu an kullanılan ayarları yeni bir ayarlar dosyası olarak yaz; eskisi yanında saklanır
    .saved = Yeni ayarlar kaydedildi
    .kept = Eski dosya, yanında { $name } adıyla saklanıyor
    .failed = Yeni ayarlar yazılamadı
read-again = Yeniden dene
    .hint = Düzeltildikten sonra yeniden oku
    .failed = Hâlâ okunamıyor
settings-read = Ayarlar dosyası yeniden okundu ve ayarları kullanılıyor
copy-path = Yolu kopyala
    .hint = Bir düzenleyicide açmak için dosyanın yolunu kopyala
    .done = Kopyalandı
files-removed = { $count } dosya kaldırıldı
undo-removing = Geri al
    .hint = Ayarları ve sonuçlarıyla birlikte eski yerlerine geri koy (Ctrl+Z)
settings-reset = Ayarlar varsayılanlarına döndü
undo-reset = Geri al
    .hint = Ayarları önceki hâline getir
folder-of = Klasörde göster
    .hint = Bulunduğu klasörü aç
presets-unreadable = Hazır ayarlarınız okunamadı, bu yüzden yalnızca yerleşik olanlar sunuluyor
files-gone = Geçen seferki { $count } dosya artık yok
session-unreadable = Geçen seferki dosyalar okunamadı, bu yüzden hiçbiri açılmadı; liste bir kenara alındı
files-unreadable = Geçen seferki { $count } dosya okunamadı ve dışarıda bırakıldı; listenin önceki hâli bir kenara alındı
session-unsaved = Açık dosyalar bir dahaki sefer için saklanamadı
nothing-to-paste = Yapıştırılacak bir şey yok
    .why = Panoda dosya, dosya yolu ya da görüntü yok
paste-failed = Pano okunamadı
    .image = Yapıştırılan görüntü kaydedilemedi
pick-failed = Dosya seçme penceresi çöktü, hiçbir şey seçilmedi


## Dropping files

drop-to-add = Eklemek için bırakın


## The system's file dialogs: their titles, which say what each is for,
## and the names of the kinds of files they show

dialog-add-files = Dönüştürülecek dosyaları ekle
dialog-add-folder = Kare klasörü ekle
dialog-add-images = Görüntüleri tek animasyon olarak ekle
dialog-inspect = İncelenecek çıkartmayı seç
dialog-ffmpeg = ffmpeg'i seç
dialog-results-folder = Sonuçların kaydedileceği yeri seç
images-filter = Görüntüler
stickers-filter = Çıkartmalar


## The list of files, on the left

settings = Ayarlar
light-theme = Açık temaya geç
dark-theme = Koyu temaya geç
main-menu = Ayarlar ve diğerleri
menu-undo = Geri al
    .hint = Dosyanın ayarlarındaki son değişiklik ya da az önce kaldırılan dosyalar
menu-redo = Yinele
    .hint = Geri alınan son değişiklik
menu-settings = Ayarlar…
inspect = Çıkartma incele…
    .hint = Bir dosyayı Telegram'ın sınırlarına göre denetle
feedback = Geri bildirim gönder
    .hint = Geliştiriciye Telegram'dan yazın: yüklüyse Telegram uygulamasında, değilse tarayıcıda
about-and-licences = Hakkında ve lisanslar…
show-input-in-folder = Klasörde göster
    .pasted = Yapıştırılan görüntünün kendi dosyası yok
shortcuts = Klavye kısayolları
add-files = Dosya ekle…
add-more = Dosya eklemenin diğer yolları
add-folder = Kare klasörü…
    .hint = Ad sırasıyla, tek animasyon olarak
add-images = Görüntüleri tek animasyon olarak…
    .hint = Seçilen görüntüler, ad sırasıyla
add-pasted = Panodan
    .hint = Kopyalanan dosyalar, yollar ya da bir görüntü
convert-all = { $count ->
    [0] Tümünü dönüştür
   *[other] Tümünü dönüştür ({ $count })
}
    .hint = Güncel sonucu olmayan her dosyayı dönüştür
remove-all = Tümünü kaldır
converting-batch = Dönüştürülüyor: { $current } / { $total }
stop-all = Tümünü durdur
    .hint = Dönüştürmeyi durdur ve bekleyen dosyaları kuyruktan çıkar
remove = Kaldır
remove-file = Listeden kaldır
convert = Dönüştür
    .hint = Çıkartmasını ya da emojisini oluştur
row-convert-again = Güncel değil: yeniden dönüştür
row-next = sıradaki
row-waiting = bekliyor, önünde { $ahead }
row-stale = { $size } · güncel değil
row-failed = başarısız
row-exists = bir dosya engel oluyor
row-stopped = durduruldu
drop-here = Videoları, GIF'leri, görüntüleri ya da Aseprite dosyalarını buraya bırakın
drop-here-note = Video, animasyonlu ya da statik bir çıkartmaya veya emojiye dönüştürmek için
paste = Yapıştır
name-pasted = yapıştırılan görüntü
name-frames = { $name } (kareler)
name-and-more = { $name } ve { $more } tane daha
no-font = { $count ->
    [1] { $codes } için ne tgradish'in kendi yazı tiplerinde ne de sistemdekilerde bir yazı tipi bulunamadı, bu yüzden koduyla gösteriliyor. Dosyanın kendisinde sorun yok
   *[other] { $codes } için ne tgradish'in kendi yazı tiplerinde ne de sistemdekilerde bir yazı tipi bulunamadı, bu yüzden kodlarıyla gösteriliyorlar. Dosyanın kendisinde sorun yok
}


## The bar under the settings of the file picked

stop = Durdur
stopping = Durduruluyor…
keep-best = En iyisini tut
    .hint = Başka kodlama denemeyi bırak ve şimdiye kadar sığan en iyisini sonuç olarak kaydet
    .nothing-fits = Henüz hiçbiri sığmadı: tutulacak kodlama yok
keeping-best = En iyisi tutuluyor…
cancel-waiting = İptal
next-to-convert = Sıradaki
waiting-for = Bekliyor, önünde { $ahead } dosya
convert-again = Yeniden dönüştür
    .changed = Güncel değil: geçerli ayarlarla yeniden oluştur
    .hint = Aynı ayarlarla yeniden oluştur
-save-settings = Geçerli ayarları hazır ayar olarak kaydet
save-preset = { -save-settings }…
    .hint = Herhangi bir dosyanın başlangıç noktası olabilecek kişisel bir hazır ayar olarak sakla
failed = Başarısız: { $error }
    .short = Başarısız — ayrıntılara bakın
in-the-way = tgradish'e ait olmayan bir dosya engel oluyor:
replace-it = Üzerine yaz
stopped = Durduruldu
show-in-folder = Klasörde göster
saves-as = Kaydedilecek:
    .hint = Sonuçların kaydedileceği yer “{ settings }” içinden ayarlanır
result-saved-as = Kaydedildi:
copy-result = Kopyala
    .hint = Telegram'da bir sohbete yapıştırmak için çıkartmayı dosya olarak kopyala
    .done = Kopyalandı
    .failed = Kopyalanamadı
replaces-earlier = Önceki bir sonucun yerine geçer
    .any = Oradaki dosyanın yerine geçer


## Reading a file to show it

reading-crashed = tgradish bunu okurken çöktü
reading-stopped = Okuma durdu
no-frames = ffmpeg içinde hiç kare bulamadı
no-frames-there = ffmpeg orada hiç kare bulamadı
no-frame-there = ffmpeg orada kare bulamadı
art-no-frames = Hiç karesi yok
result-no-frames = Sonucun hiç karesi yok


## The result's small picture over the preview

inset-label = Sonuç: { $scale }
    .times = Sonuç: { $scale }
    .plain = Sonuç
    .drag = Başka bir köşeye sürükleyin
inset-scale = { $times }×
inset-smaller = Daha küçük
    .same = Daha küçük gösterilemez
inset-larger = Daha büyük
    .same = Daha büyük gösterilemez


## Over the preview

show-input = Girdi
show-result = Sonuç
    .outdated = Sonuç ⚠
    .hint = Güncel değil: sonuç önceki ayarlarla oluşturuldu
convert-first = Önce dönüştürün
tgs-unseen = Bu TGS çıkartmada tgradish'in çizemediği şekiller var
result-preview = Sonuç önizlemesi
    .hint = Girdinin yanındaki küçük önizlemeyi göster / gizle
    .input-only = Sonucun değil, girdinin yanında gösterilir
zoom-out = Uzaklaştır
    .hint = Tamamını göster
    .all = Zaten tamamen uzaklaştırıldı
input-hint = Sürükleyerek kırp · tekerlekle yakınlaştır · sağ tuşla kaydır
crop-hint = Kırpmayı ya da kenarlarını sürükle · tümü için çift tıkla
result-hint = Tekerlekle yakınlaştır · sağ tuşla kaydır
unreadable = Okunamadı: { $error }
reading = Okunuyor…
preview-divider = Önizlemenin altındaki çizgi


## The timeline under the preview

play = Oynat
    .pause = Duraklat
play-keys =
    Boşluk — oynat ya da duraklat
    ← / → — bir kare geri ya da ileri
    Shift+← / → — on kare geri ya da ileri
    Home / End — kullanılan ilk ya da son kare
    {"["} / ] — kullanılan kısmı gösterilen karede başlat ya da bitir
    Ctrl+← / → — kullanılan kısmın başını taşı
    Alt+← / → — kullanılan kısmın sonunu taşı
nothing-to-play = Tek kare kullanılıyor: oynatılacak bir şey yok
in-seconds = Saniye
    .hint = Zamanlar saniye cinsinden
in-frames = Kare
    .hint = Zamanlar, 1'den başlayan kare numaraları olarak
    .unknown = Girdinin karelerinin nerede olduğu bilinmiyor
part-start = Kullanılan kısmın başladığı yer
    .frame = Kullanılan kısmın başladığı yer: ilk karesi
part-end = Kullanılan kısmın bittiği yer
    .frame = Kullanılan kısmın bittiği yer: son karesi
frame-number = { $number }. kare
limit-mark = Telegram'ın 3 saniyelik sınırı: kullanılan kısmın sınırı aşan bölümü kesilir
    .spoofed = Telegram'ın 3 saniyelik sınırı: Telegram'a çıkartmanın daha kısa olduğu söylendiği için kullanılan kısmın sınırı aşan bölümü de oynar
    .sped-up = Telegram'ın 3 saniyelik sınırı: daha uzun bir kullanılan kısım, sığması için daha hızlı oynar
frame-before = Önceki kare (←)
    .first = Bu ilk kare
frame-after = Sonraki kare (→)
    .last = Bu son kare
frame-picked = Kare


## What to make, beside the preview

make = Oluştur
format-video = Video
    .hint = Video çıkartma ya da emoji: WebM içinde VP9, herhangi bir videodan ya da görüntüden
format-animation = Animasyonlu
    .hint = Animasyonlu çıkartma ya da emoji: TGS dosyası. Bu seçenek piksel sanatı içindir: TGS vektörel bir biçim olduğundan her boyutta net kalır. En çok, yalnızca 100 × 100 px olan özel emojiler için uygundur. Sıradan video ya da fotoğraflar için değildir: onlar için “{ format-video }” seçeneğini kullanın
format-image = Statik
    .hint = Statik çıkartma ya da emoji: WebP, herhangi bir görüntüden ya da bir videonun tek karesinden
target-sticker = Çıkartma
target-emoji = Özel emoji
needs-webm-sticker = WebM, uzun kenarı 512 px, en fazla 256 KiB
needs-webm-emoji = WebM, 100 × 100 px, en fazla 64 KiB
needs-tgs = TGS, kare biçimli, en fazla 64 KiB
needs-image-sticker = WebP, uzun kenarı 512 px, en fazla 512 KiB
needs-image-emoji = WebP, 100 × 100 px, en fazla 512 KiB
not-pixel-art = “{ format-animation }”, net kalması için vektöre dönüştürülen piksel sanatı içindir. Bu, piksel sanatına benzemiyor: ona “{ format-video }” daha uygun
preset = Hazır ayar
    .hint = Sonuç için ne kadar uğraşılacağı; aşağıdaki kendi değerleriniz bunun üzerine uygulanır
    .image = Statik çıkartma, hazır ayardan yalnızca görüntü ayarlarını, yani nasıl kare biçime getirilip ölçekleneceğini alır: sığıyorsa kayıpsız, sığmıyorsa yine de sığan en iyi kalitede kaydedilir
    .broken = Bu hazır ayarın dosyası bozuk
    .missing = Artık “{ $name }” adlı bir hazır ayar yok: başka birini seçin
manage-presets = Yönet…
    .hint = Hazır ayarlarınızı yeniden adlandırın, açıklayın ya da silin
preset-fast = Hızlı
    .webm = Hızlı kodlayıcıyla yalnızca bit hızını sınıra göre ayarlar: saniyeler sürer ama daha bulanık
    .tgs = Bir saniyeden kısa sürer, yüzde birkaç daha büyük
preset-balanced = Dengeli
    .webm = Bit hızını sınıra göre ayarlar ve daha iyi görünebileceği yerlerde daha düşük kare hızlarını dener
    .tgs = Hızlı ile en iyi arası
preset-best = En iyi
    .webm = Dengeli gibi, en yavaş kodlayıcıyla: uzun videolarda dakikalar sürer
    .tgs = Birkaç saniye sürer, en küçük dosya
preset-described = { $name }: { $description }


## Settings: the picture

picture = Görüntü
picture-size = Boyut
once-read = Girdi okununca gösterilir
crop = Kırpma
    .hint = Girdinin kullanılacak kısmı, kendi pikselleriyle. Değiştirmek için önizlemede sürükleyin
margin-top = Üst kenar: üstten kesilen pikseller
margin-left = Sol kenar: soldan kesilen pikseller
margin-right = Sağ kenar: sağdan kesilen pikseller
margin-bottom = Alt kenar: alttan kesilen pikseller
width = Genişlik
height = Yükseklik
crop-keys = Kırpma: oklar taşır, Ctrl+oklar boyutlandırır; Shift ile on piksel
whole-picture = Tüm görüntü
    .hint = Görüntünün tamamını kullan
    .exact = Görüntünün tamamını ya da sabit ölçeğin sol üstten aldığı kadarını kullan
middle = Orta
    .hint = Ortadan, sabit ölçeğin aldığı kadarını kullan
aspect-ratio = En boy oranı
    .hint = Sürüklerken kırpmanın koruduğu genişlik/yükseklik oranı
    .square = TGS kare biçimli olmalı ve “{ square }” şu an “{ square-off }”, yani kırpmanın biçimi korunuyor: bunu seçmek onu yeniden kare biçime zorlar
shape-free = Serbest
    .hint = Herhangi bir biçim
shape-preserve = Koru
    .hint = Girdinin kendi biçimi
shape-custom = Özel
    .hint = Yazdığınız bir biçim: oran ya da sonucun boyutu olarak
    .pick = Biçim yazmak için “{ shape-custom }” seçeneğini seçin
ratio-result = Sonuç
    .fixed = Sonuç kare biçime getiriliyor ya da kenar boşlukları var, bu yüzden boyutu kırpmayı izlemiyor
    .width = Sonuç genişliği
    .height = Sonuç yüksekliği
square = Kare biçime getir
    .hint = Sonucun nasıl kare biçime getirileceği
    .tgs = Piksel sanatının nasıl kare biçime getirileceği (her TGS kare biçimli olmalı)
square-off = Kapalı
    .hint = Sonuç kırpmanın biçimini korur
    .emoji = Emoji kare biçimli olmalı
    .tgs = TGS kare biçimli olmalı, koruduğu kırpma da öyle: bunu seçmek “{ aspect-ratio }” değerini 1:1 yapar
    .kept = Piksel sanatı, “{ aspect-ratio }” ayarının kare biçimde tuttuğu kırpmanın biçimini korur (her TGS kare biçimli olmalı)
square-pad = Doldurarak
    .hint = Karenin geri kalanını saydam kenar boşlukları doldurur
    .tgs = Karenin geri kalanını saydam kenar boşlukları doldurur
square-crop = Kırparak
    .hint = Orta kısım kareyi doldurur, geri kalanı kesilir: önizleme orayı karartır
    .tgs = Orta kısım kareyi doldurur, geri kalanı kesilir
square-stretch = Gererek
    .hint = Görüntü kareye gerilir
    .tgs = Piksel sanatı kareye gerilir (pikselleri kare olmaktan çıkar)
square-exact = Sabit tam sayı ölçek görüntünün tamamını korur: bunun yerine doldurmayı seçin
scaling = Ölçekleme
    .hint = Girdinin piksellerinin sonucun piksellerine nasıl dönüştüğü
scaling-auto = Otomatik
    .hint = İki kat ya da daha fazla büyütülmüş piksel sanatında keskin pikseller, diğerlerinde yumuşak
scaling-smooth = Yumuşak
    .hint = Komşu pikseller karışır
scaling-sharp = Keskin pikseller
    .hint = Her piksel bir blok olur ve bloklar yalnızca kenarlarında karışır
scaling-integer = Tam sayı
    .hint = Her piksel aynı tam sayıda sonuç pikseline dönüşür ya da bunların tam blokları tek piksel olur; geri kalanı saydam kenar boşlukları doldurur
integer-scale = Tam sayı ölçek
    .hint = Her girdi pikselinin kaç sonuç pikseline dönüştüğü ya da kaç girdi pikselinin tek piksel oluşturduğu
    .only = Yalnızca “{ scaling-integer }” ölçeklemeyle
scale-largest = Sığan en büyük
    .hint = Kırpmayı sonucun içinde tutan en büyük tam kat
    .preset = Hazır ayar bir ölçek belirliyor
scale-fixed = Sabit
    .hint = Bu ölçek; kırpma en fazla bu ölçekle sonuca sığan kadar olabilir
smaller = Daha küçük
larger = Daha büyük
scale-field = Üç kat büyütmek için 3, üçte bire küçültmek için 1/3 yazın
    .pick = Ölçek seçmek için “{ scale-fixed }” seçeneğini seçin
scale-unused = Tam sayı ölçekler yalnızca “{ scaling-integer }” ölçeklemeyle uygulanır
scale-unsuited = Kırpma henüz bu ölçeğe uymuyor
scale-makes = { $scale } ile kırpma { $width } × { $height } px olur
scale-makes-padded = { $scale } ile kırpma { $width } × { $height } px olur; { $box_width } × { $box_height } alanın geri kalanını saydam kenar boşlukları doldurur
scale-each-way = yatayda ×{ $across }, dikeyde ×{ $down }
scale-shrinks = Kırpma sonuçtan büyük, bu yüzden keskin piksellerle küçülür: ×1/2 gibi sabit bir ölçek onu tam bloklarla küçültür
margins = Kenar boşlukları
    .hint = Tüm girdi karelerinde saydam olan kenar satırları ve sütunları
auto-crop = Boş kenar boşluklarını otomatik kırp
    .hint = Piksel sanatını, herhangi bir karede görünür pikseli olan en küçük dikdörtgene keser
    .after = Yukarıdaki kırpmadan sonra uygulanır
art-pixel = Çizim pikseli boyutu
    .hint = Bir çizim pikselinin kaç girdi pikseli kapladığı. Izgarasının dışındaki pikseller ızgaraya taşınır
art-pixel-detected = Algılanan
    .hint = Piksel sanatından bulundu: tek renkli en küçük blokları


## Settings: time

time = Zaman
used-part = Kullanılan kısım
    .hint = Girdinin kullanılan kısmının başladığı ve bittiği yer; önizlemenin altındaki zaman çizelgesi de bunu ayarlar
part-from = Başlangıç
part-to = bitiş
part-first = İlk karesi
part-last = Son karesi
all-of-input = Girdinin tamamı
part-seconds = { $from } – { $to }, toplam { $length }
part-frame = Kare { $number }, toplam { $count }
part-frames = Kareler { $first }–{ $last }, toplam { $count }
speed = Hız
    .hint = Sonucun girdiden kaç kat hızlı oynadığı
from-preset = Hazır ayardan
    .hint = Hazır ayarın belirlediği değer
from-input = Girdideki gibi
    .speed = Girdinin oynadığı hızda
    .rate = Girdinin bu hızdaki kare hızı, en fazla 60 fps
fits-3-s = 3 s'ye sığan
    .hint = Kullanılan kısmı 3 saniyeye sığdıracak kadar hızlı
fit-3-s = 3 s'ye sığdır
    .hint = Hızı, kullanılan kısım 3 saniyede oynayacak şekilde ayarla
plays = { $played } oynar
plays-cut = { $used } içinden { $kept } oynar: geri kalanı kesilir
plays-spoofed = { $played } tümüyle oynar: Telegram'a daha kısa olduğu söylenir
frame-rate = Kare hızı
    .hint = Saniyedeki kare azaldıkça her kareye daha çok bayt kalır; girdininkinden fazlası yalnızca kareleri tekrarlar
    .sequence = Dizideki karelerin ne kadar hızlı oynadığı
    .sheet = Bir dizinin ya da sprite sayfasının karelerinin ne kadar hızlı oynadığı
default-rate = Varsayılan
    .hint = { $fps } fps, çünkü bu kareler ne kadar hızlı oynadıklarını belirtmiyor
over-3-s = Sahte süre
    .hint = Telegram, 3 saniyeden uzun sürdüğünü belirten çıkartmaları reddeder
spoof-auto = Uzunsa
    .hint = 3 saniyeden uzun çıkartmalara kısa bir süre yazılır: Telegram onları kabul eder ve tamamını oynatır
spoof-always = Her zaman
    .hint = Kısa çıkartmalara da kısa süre yazılır
spoof-never = Asla: 3 s'de kes
    .hint = Meta verideki süreyi değiştirmez ve videoyu 3 saniyede keser
spoof-to = Yazılan süre
    .hint = Meta veriye yazılan ve Telegram'ın okuduğu süre
    .honest = “{ spoof-never }” seçiliyken süre değiştirilmez
default-duration = Varsayılan
    .hint = Telegram'ın kabul ettiği kısa bir süre
frame = Kare
frame-at = Kare zamanı
    .hint = Görüntünün alındığı karenin zamanı; önizlemenin altındaki zaman çizelgesi de bunu seçer


## Settings: size and quality of a WebM

size-and-quality = Boyut ve kalite
fitting = Sınıra sığdırma
    .hint = tgradish'in dosya boyutu sınırının hemen altına inmek için neyi değiştirdiği
    .lossless = Kayıpsız kodlamada ayarlanacak bit hızı ya da kalite yoktur: bunu seçmek “{ lossless-encoding }” işaretini kaldırır
fit-auto = En iyi görünüm
    .hint = Bit hızını sınıra göre ayarlar; hiçbir şey sığmadığında ya da daha az ama her biri daha çok bitli karelerin daha iyi görünebileceği yerlerde daha düşük kare hızlarını dener ve girdiye en çok benzeyeni tutar
fit-bitrate = Bit hızı
    .hint = Seçilen kare hızında bit hızını sınıra göre ayarlar
fit-crf = Kalite
    .hint = Sabit kalite değerini (CRF) sınıra göre ayarlar
fit-fps = Kare hızı
    .hint = Sabit kalitede kare hızını sınıra göre ayarlar
fit-length = Uzunluk
    .hint = Sabit kalitede, sonunu keserek uzunluğu sınıra göre ayarlar
fit-off = Kapalı
    .hint = Çok büyük çıksa bile aşağıdaki değerlerle bir kez kodlar
encoder = Kodlayıcı çabası
    .hint = Daha hızlı kodlama aynı boyutta daha kötü görünür
encoder-fast = Hızlı
    .hint = Saniyeler sürer ama aynı boyutta gözle görülür biçimde daha kötü
encoder-balanced = Dengeli
encoder-best = En iyi
    .hint = Yavaş: uzun videolarda dakikalar sürer
fitting-off = “{ fit-off }” seçiliyken bir kez kodlanır
attempts = Kodlama sınırı
    .hint = Sığdırmanın en fazla kaç kez kodlayabileceği
attempts-default = Varsayılan
    .hint = Çoğu girdide dosya boyutu sınırına yüzde birkaç yaklaşmaya yeter
attempts-times = kez
search-within = Arama aralığı
    .hint = Sığdırmanın aradığı aralık, ayarladığı değerin biriminde
    .low = En düşük
    .high = En yüksek
search-auto = Otomatik
    .hint = Sığdırma yöntemine ve girdiye uygun genişlikte
range-to = –
crf = Kalite (CRF)
    .hint = 0 (en iyi) ile 63 arasında sabit kalite
    .lossless = Kayıpsız kodlama kalite kullanmaz
    .fitted = Kaliteyi sığdırma bulur
    .bitrate = Sığdırma kalite değil, bit hızı bulur
crf-default = Varsayılan
    .hint = Çıkartma boyutlarında iyi bir kalite
lossless = Kayıpsız
lossless-encoding = Kayıpsız kodlama
    .hint = Yalnızca çok küçük ya da hareketsiz görüntülerde küçük kalır
    .switches = Yalnızca çok küçük ya da hareketsiz görüntülerde küçük kalır. Kayıpsız kodlamada ayarlanacak bit hızı ya da kalite yoktur: bunu işaretlemek sığdırmayı “{ fit-fps }” yapar
bitrate = Bit hızı
    .hint = Kalite tek başına ayarlandığında kullanılmaz
    .fitted = Bit hızını sığdırma bulur
    .constant = Bit hızıyla değil, sabit kaliteyle kodlanır
bitrate-auto = Dosya boyutu sınırına göre
    .hint = Uzunluğun izin verdiği kadar


## Settings: size and quality of a TGS animation

tgs-encoder = Kodlayıcı çabası
    .hint = Küçük bir kodlama için ne kadar uğraşılacağı
tgs-fast = Hızlı
    .hint = Bir saniyeden kısa sürer, yüzde birkaç daha büyük
tgs-balanced = Dengeli
tgs-best = En iyi
    .hint = Birkaç saniye sürer, en küçük dosya
too-large = Sığmazsa
    .hint = Piksel sanatı olduğu gibi 64 KiB'a sığmadığında ne olacağı
never-change = Piksel sanatını asla değiştirme, bunun yerine bildir
    .set = Yukarıda ayarlandığı gibi piksel sanatı asla değiştirilmez
give-up = Önce bundan vazgeç
    .hint = Piksel sanatının sığmak için değişmesi gerektiğinde: hareketinden mi, ayrıntısından mı
give-up-either = Fark etmez
    .hint = Kazandırdığı bayta göre piksel sanatını en az değiştiren
give-up-motion = Hareket
    .hint = Önce kareler birleşir ve atılır: tam ayrıntı, daha kesik hareket
give-up-detail = Ayrıntı
    .hint = Önce görüntü kabalaşır: akıcı hareket, daha az ayrıntı
ways-to-fit = Sığdırma yolları
    .hint = Sığdırmanın piksel sanatını 64 KiB'a sığdırmak için neler yapabileceği, en az göze çarpandan başlayarak
reduce-snap = Piksel ızgarasına hizala
    .hint = Başıboş pikselleri çizim pikseli ızgarasına taşır
reduce-colours = Benzer renkleri birleştir
    .hint = Neredeyse aynı olan renkler tek renk olur
reduce-specks = Benekleri kaldır
    .hint = Çevresindekilerin hepsinden farklı olan tek pikseller
reduce-frames = Benzer kareleri birleştir
    .hint = Neredeyse aynı olan kareler tek görüntü gösterir
reduce-drop = Kare at
    .hint = İki kareden birini ya da daha azını gösterir
reduce-resolution = Çözünürlüğü düşür
    .hint = Daha az ve daha büyük çizim pikselleri
reading-art = Piksel sanatını okuma
sprite-sheet = Sprite sayfası
    .hint = Eşit hücrelerden oluşan bir ızgaraya satır satır dizilmiş kareler
read-as-sheet = Sprite sayfası olarak oku
sheet-refused = Bu dosyada zaten { $frames } kare var: sprite sayfası tek bir görüntüdür
    .converted = Sprite sayfası olarak değil, olduğu gibi okundu: zaten { $frames } kare var
sheet-cells = Hücreler
    .columns = Sütunlar
    .rows = Satırlar
sheet-frames = Kareler
sheet-all = Tümü
    .hint = Sondaki saydam olanlar dışında tüm hücreler


## Settings: an Aseprite file's tags and layers

aseprite-file = Aseprite dosyası
unreadable-choices = Seçilen etiket ve katmanlarla okunamıyor
read-all = Tamamını oku
tag = Etiket
    .hint = Bir etiketin karelerini seçer: başladığı ve bittiği yere göre ya da tersine veya ileri geri oynuyorsa kendi sırasıyla
all-frames = Tüm kareler
    .hint = Dosyanın her karesi
tag-frame = Kare { $number }
tag-frames = Kareler { $from }–{ $to }
tag-reverse = { $frames }, tersine
tag-ping-pong = { $frames }, ileri ve geri
tag-pong-ping = { $frames }, geri ve ileri
layers = Katmanlar
    .hint = Hangi katmanların göründüğü, en üstteki önce; gizli bir grup, içindeki katmanları gizler
    .group-hidden = Grubu gizli
    .file = Dosyanın gösterdiği katmanlara dön


## Settings: encoder options, folded away under the rest

encoder-options = Kodlayıcı seçenekleri
vp9-options = Ada göre kodlayıcı seçenekleri, örneğin düz grafikler için tune-content = screen. ffmpeg -h encoder=libvpx-vp9 hepsini listeler
    .name = Örneğin: tune-content
    .value = Örneğin: screen
add-option = Seçenek ekle
examples = Örnekler:
example-tune-content = Düz grafikler ve ekran kayıtları: daha keskin kenarlar
example-aq-mode = Bitleri görüntünün karmaşık olduğu yerlere harcar
example-sharpness = Pürüzsüzlükten biraz ödün vererek daha çok ince ayrıntı tutar
example-arnr-strength = Kareler arasında daha az gürültü azaltma
example-g = En az her 60 karede bir anahtar kare
example-qmax = Hiçbir yerde kalite 50'den kötü olmaz
ffmpeg-arguments = ffmpeg argümanları
    .hint = Çıktıdan önce eklenen ham argümanlar, kabuktaki gibi bölünür
    .placeholder = Örneğin: -qmin 4
    .unfinished = Bir tırnak ya da ters eğik çizgi açık kaldı: kapatılana kadar argümanlar son okundukları gibi kalır
    .system = Yalnızca sistemin ffmpeg'iyle; “{ settings }” içinden seçilebilir


## What keeps a file from being converted as it is, and the ways out

blocked = Henüz dönüştürülemiyor: { $reason }
    .heading = Henüz dönüştürülemiyor
blocked-crop = kırpma görüntünün dışında
blocked-sheet = sprite sayfasının hücreleri görüntüyü tam bölmüyor
blocked-tag = dosyada “{ $tag }” etiketi yok
quoted-names = “{ $name }”
    .two = “{ $first }” ve “{ $second }”
    .last = “{ $first }” ve “{ $second }”
    .more = “{ $name }”, { $rest }
blocked-layers = { $count ->
    [1] dosyada { $layers } katmanı yok
   *[other] dosyada { $layers } katmanları yok
}
blocked-ffmpeg = ffmpeg bulunamadı
crop-outside = { $left }, { $top } noktasından başlayan { $width } × { $height } kırpma, bu { $across } × { $down } görüntünün dışında
use-whole = Tüm görüntüyü kullan
    .hint = Hiçbir şey kırpılmaz, görüntünün tamamı korunur
sheet-uneven = { $width } × { $height } görüntü, { $columns } × { $rows } eşit hücreye bölünmüyor
sheet-nearest = Tam bölen en yakın düzen: { $columns } × { $rows }
    .hint = Bu düzene en yakın, hücreleri görüntüyü tam bölen düzen
tag-missing = Bu dosyada “{ $tag }” etiketi yok
layers-missing = { $count ->
    [1] Bu dosyada { $layers } katmanı yok
   *[other] Bu dosyada { $layers } katmanları yok
}
layers-as-file = Katmanları dosyadaki gibi göster
    .hint = Katmanları dosyanın yaptığı gibi göster ve gizle
needs-ffmpeg = Bunun için ffmpeg gerekiyor, ancak bulunamadı
    .hint = ffmpeg'i sisteminizin paketlerinden ya da ffmpeg.org'dan kurun veya nerede olduğunu seçin. Bulunduğunda dosyalar yeniden okunur
choose-ffmpeg = ffmpeg seç…
    .hint = ffmpeg programını ya da ffmpeg ile ffprobe bulunan bir klasörü seç
row-attention = müdahale gerekli
    .hint = Dönüştürülmesini engelleyen bir şey var: ayarlarına bakın
row-needs-ffmpeg = ffmpeg gerekli
    .hint = Okunması ya da sonucunun oluşturulması için ffmpeg gerekiyor, ancak bulunamadı
need-attention = { $count } dosyaya müdahale gerekli


## What converting will do, beside the preview, step by step

plan-video = Videoya uygulanacaklar:
plan-picture = Görüntüye uygulanacaklar:
plan-animation = Animasyona uygulanacaklar:
plan-art = Piksel sanatına uygulanacaklar:
plan-video-frame = Videonun bir karesine uygulanacaklar:
plan-animation-frame = Animasyonun bir karesine uygulanacaklar:
plan-art-frame = Piksel sanatının bir karesine uygulanacaklar:
plan-tag = “{ $tag }” etiketiyle oynatma
plan-tag-reverse = “{ $tag }” etiketiyle, tersine oynatma
plan-tag-ping-pong = “{ $tag }” etiketiyle, ileri ve geri oynatma
plan-tag-pong-ping = “{ $tag }” etiketiyle, geri ve ileri oynatma
plan-cropped = { $width } × { $height } boyutuna kırpma
plan-frame = { $time } anındaki kareyi alma
plan-scaled = { $width } × { $height } boyutuna ölçekleme
plan-stretched-across = Yatayda ×{ $factor } germe
plan-stretched-down = Dikeyde ×{ $factor } germe
plan-middle-cut = Ortayı { $width } × { $height } boyutunda kesme
plan-padded = Kenar boşluklarıyla { $width } × { $height } boyutuna getirme
plan-trimmed = Görünür piksellerine kırpma
plan-grid = Pikselleri { $scale } × { $scale } ızgaraya hizalama
plan-square-crop = Kare biçime kırpma
plan-square-stretch = Kare biçime germe
plan-square-pad = Kenar boşluklarıyla kare biçime getirme
plan-cut = { $length } içinden { $from }–{ $to } aralığına kısaltma
plan-faster = ×{ $factor } hızlandırma
plan-slower = ×{ $factor } yavaşlatma
plan-cut-3-s = { $played } içinden 3 s'de kesme
plan-spoofed = Sahte süre yazma: { $played } tümüyle oynar
plan-spoofed-anyway = Sahte süre yazma, { $played } yeterince kısa olsa da
plan-saved = { $name } olarak kaydetme


## Settings: the same conversion as commands, folded away under the rest

commands = Komutlar
copy = Kopyala
commands-posix = Komut satırında aynı dönüştürme, POSIX kabuğu için:
commands-powershell = Komut satırında aynı dönüştürme, PowerShell için:
pasted-temporary = Yapıştırılan görüntü geçici bir klasörde tutulur ve tgradish kapanınca silinir
ffmpeg-commands = İlk kodlamanın ffmpeg komutları; sığdırma, kodlamalar arasında bit hızını ya da kaliteyi, belki de kare hızını değiştirir:
    .builtin = Yerleşik ffmpeg'in ilk kodlamada yaptıkları, sistemin ffmpeg'i için komutlar olarak; sığdırma, kodlamalar arasında bit hızını ya da kaliteyi, belki de kare hızını değiştirir:
no-ffmpeg-commands = ffmpeg komutu yok: { $why }
    .unread = girdi henüz okunmadı
    .art = ffmpeg, her dönüştürmenin piksel sanatından çizdiği bir PNG'yi okur


## The Settings dialog

save = Kaydet
saved-to = Kaydedildi: { $path }
    .kept = Kaydedildi: { $path }; okunamayan dosya { $kept } olarak saklandı
not-saved = Kaydedilemedi: { $error }
no-home = Ayarların saklanacağı bir ev klasörü yok
settings-note = Değişiklikler yapıldıkları anda uygulanır ve saklanır
reset-settings = Varsayılanlara sıfırla
    .hint = Buradaki her ayarı tgradish'in ilk hâline döndür
ffmpeg-given = Bu çalıştırma, tgradish başlatılırken verilen ffmpeg'i kullanıyor; bu ayarlar, tgradish ffmpeg verilmeden başlatıldığında geçerli olur
ffmpeg-use = Kullan
    .hint = Videolar ffmpeg ile okunur, WebM çıkartmalar da ffmpeg ile oluşturulur
ffmpeg-auto = Otomatik
    .hint = Bu derlemede varsa yerleşik ffmpeg, yoksa sistemin
ffmpeg-builtin = Yerleşik
    .hint = tgradish'in içindeki ffmpeg
    .missing = tgradish'in bu derlemesinde ffmpeg yok
ffmpeg-system = Sistemdeki
    .hint = PATH'teki ffmpeg ve ffprobe; ham ffmpeg argümanlarına izin verir
ffmpeg-program = Program
    .hint = Bir ffmpeg yürütülebilir dosyası ya da ffmpeg ile ffprobe bulunan bir klasör; yukarıdaki seçimi geçersiz kılar
    .missing = Orada hiçbir şey yok, bu yüzden kullanılan program olduğu gibi kalıyor
ffmpeg-found = { $path }, PATH'te bulundu
for-example = Örneğin: { $example }
choose = Seç…
check = Denetle
    .hint = Bu ffmpeg'in çıkartma oluşturup oluşturamadığını denetle
checking-ffmpeg = Denetleniyor…
ffmpeg-ready = ffmpeg { $version }, VP9 destekli: hazır
ffmpeg-no-vp9 = ffmpeg { $version }, libvpx ile VP9 kodlayamıyor
converting-settings = Dönüştürme
threads = İş parçacıkları
    .hint = Dönüştürmenin aynı anda en fazla kaç tane kullandığı; daha azı bilgisayarın daha çoğunu diğer işlere bırakır ve daha uzun sürer
threads-auto = Otomatik
    .hint = Dönüştürmenin her bölümü ne kadarından yararlanıyorsa o kadar
results = Sonuçlar
save-results = Kayıt yeri
next-to-input = Her girdinin yanına
in-a-folder = Bir klasöre
    .unchosen = Bir klasör seçilene kadar sonuçlar her girdinin yanına kaydedilir
    .relative = Kullanılan klasörün değişebilmesi için yolun tamamını, en baştan yazın
    .file = Bu bir klasör değil, dosya; bu yüzden kullanılan klasör olduğu gibi kalıyor
existing-files = Var olan dosyalar
    .hint = tgradish'in oluşturduğu sonuçların üzerine, yeniden oluşturulduklarında her zaman yazılır
replace-in-the-way = Engel olan her dosyanın üzerine yaz
interface = Arayüz
language = Dil
    .hint = Burada gösterilen dil; komut satırı İngilizce kalır
    .variable = { $variable } ortam değişkeniyle seçildi
as-the-system = Sistemdeki gibi
as-the-system-language = Sistemdeki gibi: { $language }
theme = Tema
    .hint = ⚙ yanındaki düğme de açık ve koyu arasında geçiş yapar
theme-light = Açık
theme-dark = Koyu
accent = Vurgu rengi
    .hint = Seçileni, üzerine gelineni ve odaktakini belirten renk
scale = Ölçek
    .hint = Metin ve denetimlerin ne kadar büyük olduğu
    .keys = { $larger } ve { $smaller } de bunu değiştirir; { $reset } %100'e döndürür
scale-smaller = Küçült
scale-larger = Büyüt
scale-reset = %100'e dön
accent-system = { as-the-system }
    .hint = Masaüstünün vurgu rengi, yoksa Breeze'in mavisi
accent-pink = Pembe
accent-red = Kırmızı
accent-orange = Turuncu
accent-yellow = Sarı
accent-green = Yeşil
accent-teal = Turkuaz
accent-blue = Mavi
accent-purple = Mor
accent-violet = Menekşe
accent-grey = Gri
accent-own = Özel renk
    .chosen = Özel renk, { $colour }
accent-shade = Ton
    .value = { $saturation } doygunluk, { $value } parlaklık
accent-hue = Renk özü
accent-hex = Hex
    .wrong = # ve altı onaltılık basamakla yazın, örneğin #3daee9
files = Dosyalar
reopen-files = Geçen seferki dosyaları ayarlarıyla birlikte yeniden aç
motion = Hareket
    .hint = Sistem daha az hareket istediğinde, bu ayar ne olursa olsun hiçbir şey hareket etmez
motion-on = Kaydırmayı yumuşat, değişikliklerde solma ve kayma geçişleri kullan
new-files = Yeni dosyalar
preset-for-webm = WebM için hazır ayar
preset-for-tgs = TGS için hazır ayar


## The result, beside the preview

last-conversion = Son dönüştürme
path-in-the-way = { $path } engel oluyor
stopped-early = Bitmeden durduruldu
result = Sonuç
    .stale = Sonuç, güncel değil
result-copy = Telegram'da bir sohbete yapıştırmak için kopyalayın ({ $keys })
result-webm = { $width } × { $height } px, { $fps } fps, { $length }
    .spoofed = { $width } × { $height } px, { $fps } fps, { $length }, sahte süre
changed-to-fit = Sığması için değiştirildi: nasıl olduğu aşağıda
replaced-earlier = tgradish'in oluşturduğu önceki bir sonucun yerine geçti
converting = Dönüştürülüyor
stage-reading = Piksel sanatı okunuyor
stage-drawing = Vektöre dönüştürülüyor
stage-fitting-steps = 64 KiB'a sığdırılıyor: { $count } adım, yaklaşık { $size }
stage-fitting-from = { $size } boyutundan 64 KiB'a sığdırılıyor
stage-compressing = Sıkıştırılıyor
    .hopeless = Hızlıca sıkıştırılıyor: sığamaz
    .missed = Sıkıştırıldı → { $size }, çok büyük
    .missed-far = Sıkıştırılıyor: hâlâ fazlasıyla büyük
retry = { $number }. deneme: hedef { $aim }
retry-done = { $number }. deneme: hedef { $aim } → { $size }, çok büyük
    .far = { $number }. deneme: hedef { $aim } → hâlâ fazlasıyla büyük
starting = Başlatılıyor
encode = { $number }. kodlama: { $fps } fps, { $rate }
encode-of = { $number }. kodlama (en fazla { $most }): { $fps } fps, { $rate }
encode-done = { $number }. kodlama: { $fps } fps, { $rate } → { $size }, sığıyor
    .too-large = { $number }. kodlama: { $fps } fps, { $rate } → { $size }, çok büyük
rate-quality = kalite { $crf }
rate-lossless = kayıpsız
encodes-up-to = Biri dosya boyutu sınırının hemen altına inene kadar en fazla { $count } kodlama
art-size = { $width } × { $height } çizim pikseli
    .scaled = { $width } × { $height } çizim pikseli, her biri { $scale } × { $scale } girdi pikseli
art-summary = { $size }; { $colours }, { $frames }
art-tempo = Girdinin ×{ $tempo } hızında oynar
art-off-grid = Çoğu, bazı pikselleri ızgara dışında kalan { $scale }× piksel sanatına benziyor; { $scale } çizim pikseli boyutu onları ızgaraya hizalar
art-shapes = { $layers }, { $rectangles }; açılmış hâli { $size }
art-before-fitting = Olduğu gibi yaklaşık { $size }; sığması için yapılan değişiklikler:
art-step = { $change }, ardından yaklaşık { $size }
step-snapped = pikseller { $scale } ızgarasına hizalandı
step-colours = farkı { $distance } altında olan renkler birleştirildi
step-frames = { $changed } altında değişen kareler birleştirildi
step-dropped = karelerin { $share } kadarı atıldı
step-specks = tek başıboş pikseller kaldırıldı
step-resolution = çözünürlük { $factor } oranına düşürüldü
column-number = Sırayla her kodlama; ✔ tutulan
column-fps = Saniyedeki kare
column-rate = Bit hızı ya da kodlandığı kalite
column-size = boyut
    .hint = Dosya boyutu sınırına oranı, sığıyorsa yeşil
column-alike = benzerlik
    .hint = Girdiye ne kadar benzediği, SSIM (yapısal benzerlik) ile ölçülür
rate-quality-short = q { $crf }
decided-in-band = Durdu: { $number }. kodlama sınırın { $share } kadarını kullanıyor, hedeflenen en az { $least } karşılandı
decided-alike-enough = Durdu: { $number }. kodlama girdiye zaten { $alike } benziyor, daha çok bit onu yalnızca büyütür
decided-alike-unchanged = Durdu: { $number }. kodlama girdiye { $alike } benziyor, daha az bitli { $than }. kodlamadan pek fazla değil
decided-little-to-gain = Durdu: { $number }. kodlama sınırın { $share } kadarını kullanıyor, bir sonraki yalnızca yaklaşık { $predicted } kullanırdı
decided-too-large-at-least = Durdu: aralığın en düşüğü bile, { $number }. kodlamada, sınırın { $share } kadarını kullanıyor
decided-nothing-left = Durdu: denemeye değer ayar kalmadı
decided-out-of-attempts = { $count ->
    [one] Durdu: izin verilen { $count } kodlama yapıldı
   *[other] Durdu: izin verilen { $count } kodlamanın tümü yapıldı
}
decided-lowers-to-fit = { $fps } fps deneniyor: { $from } fps'de hiçbiri sığmadı
decided-lowers-for-looks = { $fps } fps deneniyor: { $from } fps'deki { $number }. kodlama girdiye yalnızca { $alike } benziyor
decided-settled = Erken durdu: { $number }. kodlama tutuldu
decided-lower-no-better = Durdu: { $fps } fps girdiye { $alike } benziyor, { $kept_fps } fps'deki { $kept }. kodlamanın { $kept_alike } benzerliğinden fazla değil
issue-not-webm = Kapsayıcısı WebM değil, { $container }
issue-no-video = Video izi yok
issue-not-vp9 = Kodeki VP9 değil, { $codec }
issue-audio = Ses izi var
issue-size = Boyutu { $width } × { $height } px, oysa Telegram { $side } × { $side } px kabul ediyor
    .sticker = Boyutu { $width } × { $height } px, oysa Telegram bir kenarı { $side } px, diğeri en fazla o kadar olanı kabul ediyor
issue-too-big = Boyutu { $size }, { $limit } sınırını aşıyor
issue-too-long = Meta verisine göre { $seconds } sürüyor, { $limit } sınırını aşıyor
issue-too-many-fps = Kare hızı { $fps } fps, { $limit } fps sınırını aşıyor
issue-animated = Hareketli, oysa statik çıkartmalar tek bir görüntüdür
issue-frame-rate = Kare hızı { $fps } fps, oysa Telegram { $required } fps kabul ediyor
    .accepted = Kare hızı { $fps } fps: Telegram'ın kurallarına aykırı, ama Telegram yine de kabul ediyor
issue-no-frames = Hiç karesi yok
issue-unpacked = Açılmış hâlde boyutu { $size }, Telegram Desktop'ın { $limit } sınırını aşıyor
issue-count =
    .layers = Katmanlar: { $count }, { $limit } sınırını aşıyor
    .shapes-per-layer = Bir katmanda en çok şekil: { $count }, { $limit } sınırını aşıyor
    .paint-points = Bir dolguda en çok yol noktası: { $count }, { $limit } sınırını aşıyor
    .paints-per-layer = Bir katmanda en çok dolgu ve kontur: { $count }, Telegram uygulamalarının { $limit } sınırını aşıyor
    .paint-sources = Bir katmanda en çok boyanan şekil: { $count }, Telegram uygulamalarının { $limit } sınırını aşıyor
    .path-points = Bir yolda en çok nokta: { $count }, Telegram uygulamalarının { $limit } sınırını aşıyor
    .keyframes = Bir özellikte en çok anahtar kare: { $count }, Telegram uygulamalarının { $limit } sınırını aşıyor
    .assets = Varlıklar: { $count }, Telegram uygulamalarının { $limit } sınırını aşıyor
    .expanded-layers = Ön kompozisyonlar açılınca katmanlar: { $count }, Telegram uygulamalarının { $limit } sınırını aşıyor
    .nesting = JSON'undaki iç içe geçme düzeyleri: { $count }, Telegram uygulamalarının { $limit } sınırını aşıyor
    .group-depth = İç içe grup düzeyleri: { $count }, Telegram uygulamalarının { $limit } sınırını aşıyor
issue-cost = Çizim maliyeti: { $cost }, { $limit } sınırını aşıyor; şekiller: { $shapes }, katmanlar: { $layers }, her biri { $each }
issue-empty-tangents = Telegram'ın reddettiği boş teğetli ([]) yollar: { $count }; [0,0] çalışır
issue-path-coordinate = En büyük yol koordinatı: { $coordinate }, Telegram uygulamalarının { $limit } sınırını aşıyor
issue-precomp-loop = Ön kompozisyonları döngü hâlinde birbirine başvuruyor
issue-feature = { $feature }: Telegram'ın kurallarına aykırı, ama Telegram yine de kabul ediyor
    .undrawn = { $feature }: Telegram'ın kurallarına aykırı; Telegram kabul ediyor ama uygulamaları bunu çizmiyor
    .ignored = { $feature }: Telegram uygulamaları bunu çizmiyor
warned-fewer-frames = { $fps } fps ile okundu, çünkü { $width } × { $height } boyutunda daha fazla kare belleğe sığmazdı
warned-many-colours = { $colours } içeriyor; bu piksel sanatı için alışılmadık, bu yüzden çıkartma büyük olacak
warned-length-capped = Sığdırmanın denediği süreler { $length } ile sınırlandı, çünkü sığdırma yalnızca kısaltır
warned-input-shorter = Girdi, seçilen başlangıçtan sonra yalnızca { $length } sürüyor: kalanın tamamı kullanılıyor
warned-trimmed = { $length } uzunluğa kısaltıldı: sahte süre kapalı ve Telegram daha uzun videoları reddediyor
warned-left-out = Meta verisinde tgradish'in adı olmadan yazıldı, çünkü ham ffmpeg argümanları ona yer bırakmadı: { $details }
    .duration = Meta verisinde sahte süre olmadan yazıldı, çünkü ham ffmpeg argümanları ona yer bırakmadı: { $details }


## Inspecting a sticker

inspect-another = Başkasını incele…
inspect-webm-sticker = Video çıkartma
inspect-webm-emoji = Video emoji
inspect-image-sticker = Statik çıkartma
inspect-image-emoji = Statik emoji
inspect-tgs = Vektör animasyon, { $width } × { $height }, { $fps } fps
inspect-unknown = Bu sürümün henüz gösteremediği bir çıkartma türü
inspect-size = Boyut
    .hint = Telegram daha büyük dosyaları reddeder
    .tgs = Telegram daha büyük çıkartmaları reddeder
inspect-duration = Süre
    .hint = Telegram süreyi meta veriden okur; sahte süre onu kısaltır
    .spoofed = Meta veride { $seconds } s, sahte
    .no-header = { $seconds }, meta veride yok
inspect-plays = Oynama süresi
    .hint = Videonun gerçekte ne kadar uzun olduğu
inspect-frame-rate = Kare hızı
    .value = { $fps } fps (en fazla { $most }), { $frames }
inspect-picture = Görüntü
    .none = yok
inspect-video = { $width } × { $height } px, { $codec }
    .alpha = { $width } × { $height } px, { $codec }, saydamlıklı
inspect-webp = { $width } × { $height } px, WebP, kayıpsız
    .alpha = { $width } × { $height } px, WebP, kayıpsız, saydamlıklı
    .lossy = { $width } × { $height } px, WebP, kayıplı
    .lossy-alpha = { $width } × { $height } px, WebP, kayıplı, saydamlıklı
inspect-audio = Ses
inspect-title = Başlık
inspect-made-with = Oluşturan
    .hint = Dosyaya göre, onu yazan programlar
    .image = Meta verisinde adı geçen program
    .unknown = bilinmiyor
inspect-note = Not
    .hint = Dosyanın dolgusundaki, oynatıcıların atladığı metin
inspect-name = Ad
    .hint = Animasyonun kendi adı; Telegram bunu göstermez
inspect-length = Uzunluk
    .hint = Animasyonlu çıkartmalar en fazla 3 saniye sürer
    .value = { $seconds } / 3.00 s, { $frames }
inspect-cost = Çizim maliyeti
    .hint = Telegram'ın saydığı gibi, şekiller artı her katman için 9
inspect-layers = Katmanlar
inspect-shapes = Bir katmanda en çok şekil
inspect-refuses-more = Telegram fazlasını reddeder
inspect-points = Bir dolguda en çok nokta
    .hint = Tek bir dolgunun boyadığı yol noktaları; Telegram fazlasını reddeder
inspect-unpacked = Açılmış
    .hint = Telegram Desktop daha büyük animasyonları oynatmaz
inspect-features = Özellikler
    .hint = Şekiller ve dönüşümlerin ötesindeki Lottie özellikleri


## What went wrong, as converting or reading a file reports it

crashed = tgradish çöktü: { $details }
ffmpeg-not-found = ffmpeg bulunamadı: { $details }
ffmpeg-not-on-path = ffmpeg ve ffprobe PATH'te yok: onları kurun ya da “{ settings }” içinden başka bir ffmpeg seçin
ffmpeg-args-need-program = ffmpeg argümanları için sistemin ffmpeg'i gerekir, bu da “{ settings }” içinden seçilebilir; kodlayıcı seçenekleri ikisiyle de çalışır
program-failed = { $program } başarısız oldu ({ $status }):
unreadable-file = { $name } okunamadı: { $details }
no-video = { $name } içinde video yok
no-such-file = { $name } artık orada değil
not-webm = { $name } bir WebM dosyası değil
not-sticker = { $name } bir çıkartma, yani WebM, WebP ya da .tgs dosyası değil
no-frame-at = { $name } içinde { $seconds } anında kare yok
nothing-fits = { $limit } sınırına hiçbir şey sığmıyor: en küçük kodlama { $smallest } idi. Daha kısa bir kullanılan kısım, daha düşük bir kare hızı ya da daha geniş bir arama aralığı işe yarayabilir
clipboard-failed = Panodan yapıştırılamadı: { $details }
output-is-input = Sonuç, kendisinden oluşturulduğu { $name } dosyasının üzerine yazılırdı
nothing-to-convert = Dönüştürülecek görüntü yok


## Saving and managing presets

save-preset-title = { -save-settings }
save-preset-note = Bu dosyanın ayarları, her dosyanın başlangıç noktası olarak; yalnızca bu dosyaya özgü olanlar hariç: çıkartma ya da emoji, kırpma, kullanılan kısım, etiket, katmanlar ve sprite sayfası
preset-name = Ad
    .placeholder = çıkartmalarım
preset-description = Açıklama
    .placeholder = isteğe bağlı
    .none = açıklama yok
preset-exists = Bu adda bir hazır ayar zaten var: kaydetmek onun yerine geçer
replace = Üzerine yaz
name-it-first = Önce ad verin
cancel = İptal
preset-name-length = Hazır ayar adı 1 ile { $most } karakter arasında olur ve boşlukla başlamaz
preset-name-characters = Hazır ayar adında yalnızca harf, rakam, boşluk, - ve _ olabilir
preset-name-builtin = “{ $name }” yerleşik bir hazır ayarın adı
preset-name-taken = Başka bir hazır ayarın adı “{ $name }”
preset-extended = “{ $other }” bunu temel alıyor: önce o hazır ayarı değiştirin
your-presets = Hazır ayarlarınız
your-presets-note = Değişiklikler yaptığınız anda saklanır. “{ preset-fast }”, “{ preset-balanced }” ve “{ preset-best }” yerleşiktir ve olduğu gibi kalır
no-presets = Henüz yok: ayarların altındaki çubukta “{ -save-settings }”
preset-broken = Dosyası okunamıyor
delete-it = Sil
deleted = “{ $name }” silindi
undo = Geri al
    .hint = Geri getir
not-put-back = “{ $name }” hazır ayarı geri getirilemedi


## About and licences

about-title = Hakkında ve lisanslar
about-summary = Videolardan, görüntülerden ya da piksel sanatından Telegram çıkartmaları ve emojileri. MIT lisansı;
source-code = kaynak kodu
send-feedback = Telegram'dan geri bildirim gönder
about-notices = tgradish başkalarının emeğiyle de yapıldı. Onların lisansları bu bildirimlerin her kopyayla birlikte verilmesini istiyor, bu yüzden program onları taşıyor:
notice-licence = { $name }: lisans
notice-named-licence = { $name }: { $licence }
notice-licence-notes = { $name }: lisans notları
notice-patent-grant = { $name }: patent izni
notice-sources = { $name }: yerleşik olanlar ve kaynakları
notice-font = { $subsets ->
    [1] Yazı tipi: { $name }, bir alt küme
   *[other] Yazı tipi: { $name }, alt kümeler
}
notice-crates = Rust crate'leri
notice-crates-licence = { $licence }: { $crates }
    .more = { $licence }: { $crates } ve { $more } tane daha
ffmpeg-sources = Yerleşik ffmpeg'in bu sürümle birlikte gelen tam kaynakları


## The dialog of keyboard shortcuts, opened from the menu or by ? or F1

key-ctrl = Ctrl
key-cmd = Cmd
key-alt = Alt
key-shift = Shift
key-enter = Enter
key-space = Boşluk
key-delete = Delete
key-backspace = Backspace
key-home = Home
key-end = End
key-escape = Esc
key-left = Sol
key-right = Sağ
key-up = Yukarı
key-down = Aşağı
shortcuts-files = Dosyalar
shortcuts-playing = Önizleme ve zaman çizelgesi
shortcuts-general = Genel
shortcuts-mouse = Fare, önizlemede ve zaman çizelgesinde
shortcut-add-files = Dosya ekle
shortcut-paste = Panodaki dosyaları ya da görüntüyü ekle
shortcut-select = Üstteki ya da alttaki dosyayı seç
shortcut-convert = Seçili dosyayı dönüştür
shortcut-convert-all = Tümünü dönüştür: güncel sonucu olmayan her dosya
shortcut-keep-best = WebM sığdırılırken şimdiye kadarki en iyi kodlamayı tut
shortcut-copy-result = Telegram'a yapıştırmak için seçili dosyanın sonucunu kopyala
shortcut-remove = Seçili dosyayı ya da satırı odakta olanı kaldır
shortcut-undo = Seçili dosyadaki bir değişikliği geri al ya da kaldırılan dosyaları geri getir
shortcut-redo = Geri alınan bir değişikliği yinele
shortcut-play = Oynat ya da duraklat
shortcut-step = Bir kare geri ya da ileri; Shift ile on
shortcut-ends = Kullanılan ilk ya da son kare
shortcut-cut-here = Kullanılan kısmı gösterilen karede başlat ya da bitir
shortcut-move-start = Kullanılan kısmın başını taşı; Shift ile on kare
shortcut-move-end = Kullanılan kısmın sonunu taşı; Shift ile on kare
shortcut-settings = Ayarları aç
shortcut-inspect = Çıkartma incele
shortcut-scale-up = Her şeyi büyüt
shortcut-scale-down = Her şeyi küçült
shortcut-scale-reset = %100'e dön
shortcut-shortcuts = Bu liste
shortcut-close = Bir pencereyi ya da menüyü kapat
with-keys = { $hint } ({ $keys })
mouse-drag = Sürükle
    .does = Kırp ya da kırpmayı veya kenarlarını taşı
mouse-shape = Köşeyi Shift+sürükle
    .does = En boy oranı ayarlı değilse kırpmanın biçimini koru
mouse-centred = Kenarı Alt+sürükle
    .does = Kırpmayı ortasına göre boyutlandır
mouse-free = Ctrl+sürükle
    .does = Görüntünün kenarlarına ve ortasına yapışmadan kırp
mouse-cancel = Sürüklerken Esc
    .does = Kırpmayı eski hâline getir
mouse-crop-keys = Kırpmaya tıkla, sonra oklar
    .does = Bir piksel taşı, Shift ile on; Ctrl+oklar boyutlandırır
mouse-double-click = Çift tıkla
    .does = Tüm görüntüye dön
mouse-wheel = Tekerlek
    .does = Yakınlaştır ya da uzaklaştır
mouse-pan = Sağ ya da orta tuşla sürükle
    .does = Yakınlaştırılmışken kaydır
mouse-pan-across = Shift+tekerlek
    .does = Yakınlaştırılmışken yatay kaydır
mouse-slide = Köşeli ayracı Shift+sürükle
    .does = Zaman çizelgesindeki kullanılan kısmı, uzunluğunu koruyarak kaydır
