### tgradish's words in Indonesian. Where each message is shown and what
### it means is said in the comments of the English file,
### en-GB/tgradish.ftl.

language-name = Bahasa Indonesia


## Shared by many places

close = Tutup
back-to-preset = Kembalikan ke nilai preset
custom = Kustom
    .hint = Pilih “{ custom }” untuk mengaturnya
webm-sticker = Stiker WebM
webm-emoji = Emoji WebM
tgs-sticker = Stiker TGS
tgs-emoji = Emoji TGS
image-sticker = Stiker statis
image-emoji = Emoji statis
count-colours = { $count } warna
count-frames = { $count } frame
count-layers = { $count } lapisan
count-rectangles = { $count } persegi panjang
count-tracks = { $count } trek
of-limit = { $used } dari { $limit }
telegram-accepts = Telegram seharusnya menerimanya


## Messages along the bottom

dismiss = Tutup
    .all = Tutup semua
said-times = ×{ $count }
details = Detail
settings-unreadable = File pengaturan tidak dapat dibaca, jadi pengaturan default yang dipakai
setting-unsaved = Perubahan tidak dapat disimpan ke file pengaturan
    .unreadable = Perubahan tidak disimpan: file pengaturan tidak dapat dibaca
settings-file-note = Mungkin file ini diedit secara manual atau oleh versi tgradish lain. Perubahan tidak dapat disimpan sampai file ini diperbaiki atau diganti; isinya tidak ada yang hilang.
fresh-settings = Mulai dengan pengaturan baru
    .hint = Tulis pengaturan yang sedang dipakai sebagai file pengaturan baru; file lama disimpan di sebelahnya
    .saved = Pengaturan baru telah disimpan
    .kept = File lama disimpan sebagai { $name } di sebelahnya
    .failed = Pengaturan baru tidak dapat ditulis
read-again = Coba lagi
    .hint = Baca ulang setelah file diperbaiki
    .failed = Masih tidak dapat dibaca
settings-read = File pengaturan sudah dapat dibaca lagi, dan pengaturannya kini dipakai
copy-path = Salin jalur
    .hint = Salin lokasi file untuk membukanya di editor
    .done = Disalin
files-removed = { $count } file dihapus dari daftar
undo-removing = Urungkan
    .hint = Kembalikan ke tempat semula, beserta pengaturan dan hasilnya (Ctrl+Z)
settings-reset = Pengaturan kembali ke default
undo-reset = Urungkan
    .hint = Kembalikan pengaturan seperti sebelumnya
folder-of = Tampilkan di folder
    .hint = Buka folder tempat file ini berada
presets-unreadable = Preset Anda tidak dapat dibaca, jadi hanya preset bawaan yang tersedia
files-gone = { $count } file dari sesi terakhir sudah tidak ada
session-unreadable = Daftar file sesi terakhir tidak dapat dibaca, jadi tidak ada yang dibuka; daftar itu disisihkan
files-unreadable = { $count } file dari sesi terakhir tidak dapat dibaca dan dilewati; daftar aslinya disisihkan
session-unsaved = File yang terbuka tidak dapat disimpan untuk sesi berikutnya
nothing-to-paste = Tidak ada yang bisa ditempel
    .why = Papan klip tidak berisi file, jalur file, atau gambar
paste-failed = Papan klip tidak dapat dibaca
    .image = Gambar yang ditempel tidak dapat disimpan
pick-failed = Dialog file mengalami crash, jadi tidak ada yang dipilih


## Dropping files

drop-to-add = Lepaskan untuk menambahkan file


## The system's file dialogs: their titles, which say what each is for,
## and the names of the kinds of files they show

dialog-add-files = Tambahkan file untuk dikonversi
dialog-add-folder = Tambahkan folder frame
dialog-add-images = Tambahkan gambar sebagai satu animasi
dialog-inspect = Pilih stiker untuk diperiksa
dialog-ffmpeg = Pilih ffmpeg
dialog-results-folder = Pilih lokasi penyimpanan hasil
images-filter = Gambar
stickers-filter = Stiker


## The list of files, on the left

settings = Pengaturan
light-theme = Beralih ke tema terang
dark-theme = Beralih ke tema gelap
main-menu = Pengaturan dan lainnya
menu-undo = Urungkan
    .hint = Perubahan terakhir pada pengaturan file, atau file yang baru dihapus
menu-redo = Ulangi
    .hint = Perubahan terakhir yang diurungkan
menu-settings = Pengaturan…
inspect = Periksa stiker…
    .hint = Periksa file terhadap batas Telegram
feedback = Kirim masukan
    .hint = Tulis ke pembuatnya di Telegram: di aplikasinya jika terpasang, atau di browser
about-and-licences = Tentang dan lisensi…
show-input-in-folder = Tampilkan di folder
    .pasted = Gambar yang ditempel tidak punya file sendiri
shortcuts = Pintasan keyboard
add-files = Tambah file…
add-more = Cara lain menambahkan file
add-folder = Folder frame…
    .hint = Diurutkan menurut nama, sebagai satu animasi
add-images = Gambar sebagai satu animasi…
    .hint = Gambar yang dipilih, diurutkan menurut nama
add-pasted = Dari papan klip
    .hint = File, jalur, atau gambar yang disalin
convert-all = { $count ->
    [0] Konversi semua
   *[other] Konversi semua ({ $count })
}
    .hint = Konversi setiap file yang belum punya hasil terbaru
remove-all = Hapus semua
converting-batch = Mengonversi { $current } dari { $total }
stop-all = Hentikan semua
    .hint = Hentikan konversi, dan keluarkan file yang menunggu dari antrean
remove = Hapus
remove-file = Hapus dari daftar
convert = Konversi
    .hint = Buat stiker atau emojinya
row-convert-again = Usang: konversi lagi
row-next = berikutnya
row-waiting = menunggu, { $ahead } di depan
row-stale = { $size } · usang
row-failed = gagal
row-exists = ada file yang menghalangi
row-stopped = dihentikan
drop-here = Lepaskan video, GIF, gambar, atau file Aseprite di sini
drop-here-note = Untuk dikonversi menjadi stiker atau emoji video, animasi, atau statis
paste = Tempel
name-pasted = gambar yang ditempel
name-frames = { $name } (frame)
name-and-more = { $name } dan { $more } lainnya
no-font = Tidak ada font untuk { $codes }, baik di antara font tgradish maupun font sistem, jadi ditampilkan sebagai kode. Filenya sendiri tidak bermasalah


## The bar under the settings of the file picked

stop = Hentikan
stopping = Menghentikan…
keep-best = Simpan yang terbaik
    .hint = Berhenti mencoba enkode lain, dan simpan yang terbaik yang muat sejauh ini sebagai hasil
    .nothing-fits = Belum ada yang muat: tidak ada enkode untuk disimpan
keeping-best = Menyimpan yang terbaik…
cancel-waiting = Batal
next-to-convert = Dikonversi berikutnya
waiting-for = Menunggu { $ahead } file lagi
convert-again = Konversi lagi
    .changed = Usang: buat ulang dengan pengaturan saat ini
    .hint = Buat ulang dengan pengaturan yang sama
-save-settings = Simpan pengaturan sebagai preset
save-preset = { -save-settings }…
    .hint = Simpan sebagai preset Anda sendiri, yang bisa dipakai file mana pun sebagai titik awal
failed = Gagal: { $error }
    .short = Gagal — lihat detail
in-the-way = Ada file bukan dari tgradish yang menghalangi:
replace-it = Ganti
stopped = Dihentikan
show-in-folder = Tampilkan di folder
saves-as = Akan disimpan sebagai
    .hint = Lokasi hasil diatur di “{ settings }”
result-saved-as = Disimpan sebagai
copy-result = Salin
    .hint = Salin stiker sebagai file, untuk ditempel ke obrolan Telegram
    .done = Disalin
    .failed = Gagal disalin
replaces-earlier = Mengganti hasil sebelumnya
    .any = Mengganti file yang ada di sana


## Reading a file to show it

reading-crashed = tgradish mengalami crash saat membaca ini
reading-stopped = Pembacaan dihentikan
no-frames = ffmpeg tidak menemukan frame di dalamnya
no-frames-there = ffmpeg tidak menemukan frame di sana
no-frame-there = ffmpeg tidak menemukan frame di titik itu
art-no-frames = Tidak ada frame di dalamnya
result-no-frames = Hasilnya tidak punya frame


## The result's small picture over the preview

inset-label = Hasil ({ $scale })
    .plain = Hasil
    .drag = Seret ke sudut lain
inset-scale = { $percent }%
    .times = { $times }×
inset-smaller = Lebih kecil
    .same = Tidak bisa tampil lebih kecil
inset-larger = Lebih besar
    .same = Tidak bisa tampil lebih besar


## Over the preview

show-input = Input
show-result = Hasil
    .outdated = Hasil ⚠
    .hint = Usang: hasil dibuat dengan pengaturan sebelumnya
convert-first = Konversi dulu
tgs-unseen = Stiker TGS ini punya bentuk yang tidak bisa digambar tgradish
result-preview = Pratinjau hasil
    .hint = Tampilkan / sembunyikan pratinjau kecil di samping input
    .input-only = Ditampilkan di samping input, bukan hasil
zoom-out = Perkecil
    .hint = Tampilkan semuanya
    .all = Sudah diperkecil sepenuhnya
input-hint = Seret untuk memangkas · roda gulir untuk zoom · tombol kanan untuk menggeser
crop-hint = Seret area pangkas atau tepinya · klik ganda untuk seluruh gambar
result-hint = Roda gulir untuk zoom · tombol kanan untuk menggeser
unreadable = Tidak dapat dibaca: { $error }
reading = Membaca…
preview-divider = Garis di bawah pratinjau


## The timeline under the preview

play = Putar
    .pause = Jeda
play-keys =
    Spasi — putar atau jeda
    ← / → — mundur atau maju satu frame
    Shift+← / → — mundur atau maju sepuluh frame
    Home / End — frame pertama atau terakhir yang dipakai
    {"["} / ] — mulai atau akhiri bagian terpakai di frame yang tampil
    Ctrl+← / → — geser awal bagian terpakai
    Alt+← / → — geser akhir bagian terpakai
nothing-to-play = Hanya satu frame yang dipakai: tidak ada yang bisa diputar
in-seconds = Detik
    .hint = Waktu dalam detik
in-frames = Frame
    .hint = Waktu sebagai nomor frame, mulai dari 1
    .unknown = Letak frame input tidak diketahui
part-start = Awal bagian terpakai
    .frame = Awal bagian terpakai: frame pertamanya
part-end = Akhir bagian terpakai
    .frame = Akhir bagian terpakai: frame terakhirnya
frame-number = frame { $number }
limit-mark = Batas 3 detik Telegram: bagian terpakai setelahnya dipotong
    .spoofed = Batas 3 detik Telegram: bagian terpakai setelahnya tetap diputar, karena Telegram diberi tahu stikernya lebih pendek
    .sped-up = Batas 3 detik Telegram: bagian terpakai yang lebih panjang diputar lebih cepat agar muat
frame-before = Frame sebelumnya (←)
    .first = Ini frame pertama
frame-after = Frame berikutnya (→)
    .last = Ini frame terakhir
frame-picked = Frame


## What to make, beside the preview

make = Buat
format-video = Video
    .hint = Stiker atau emoji video: VP9 dalam WebM, dari video atau gambar apa pun
format-animation = Animasi
    .hint = Stiker atau emoji animasi: file TGS. Opsi ini ditujukan untuk pixel art: TGS adalah format vektor, jadi tetap tajam di ukuran berapa pun. Paling cocok untuk emoji kustom, yang hanya 100 × 100 px. Bukan untuk video atau foto biasa: gunakan “{ format-video }” untuk itu
format-image = Statis
    .hint = Stiker atau emoji statis: WebP, dari gambar apa pun atau satu frame video
target-sticker = Stiker
target-emoji = Emoji kustom
needs-webm-sticker = WebM, 512 px di sisi terpanjang, hingga 256 KiB
needs-webm-emoji = WebM, 100 × 100 px, hingga 64 KiB
needs-tgs = TGS, persegi, hingga 64 KiB
needs-image-sticker = WebP, 512 px di sisi terpanjang, hingga 512 KiB
needs-image-emoji = WebP, 100 × 100 px, hingga 512 KiB
not-pixel-art = “{ format-animation }” ditujukan untuk pixel art, yang diubah menjadi vektor agar tetap tajam. Ini tidak terlihat seperti pixel art: “{ format-video }” lebih cocok
preset = Preset
    .hint = Tingkat upaya untuk hasilnya; nilai Anda sendiri di bawah akan menimpanya
    .image = Stiker statis hanya mengambil pengaturan gambar dari preset, yaitu cara dijadikan persegi dan diskalakan: disimpan lossless jika muat, jika tidak dengan kualitas terbaik yang masih muat
    .broken = File preset ini rusak
    .missing = Preset “{ $name }” sudah tidak ada: pilih yang lain
manage-presets = Kelola…
    .hint = Ganti nama, beri deskripsi, atau hapus preset Anda
preset-fast = Cepat
    .webm = Hanya menyesuaikan laju bit, dengan enkoder cepat: hitungan detik, tapi lebih buram
    .tgs = Kurang dari satu detik, beberapa persen lebih besar
preset-balanced = Seimbang
    .webm = Menyesuaikan laju bit, dan mencoba frame rate lebih rendah jika bisa terlihat lebih baik
    .tgs = Di antara cepat dan terbaik
preset-best = Terbaik
    .webm = Seperti seimbang, dengan enkoder paling lambat: butuh beberapa menit untuk video panjang
    .tgs = Beberapa detik, file terkecil
preset-described = { $name }: { $description }


## Settings: the picture

picture = Gambar
picture-size = Ukuran
once-read = Ditampilkan setelah input dibaca
crop = Pangkas
    .hint = Bagian input yang dipakai, dalam piksel input. Seret di pratinjau untuk mengubahnya
margin-top = Margin atas: piksel yang dipangkas dari atas
margin-left = Margin kiri: piksel yang dipangkas dari kiri
margin-right = Margin kanan: piksel yang dipangkas dari kanan
margin-bottom = Margin bawah: piksel yang dipangkas dari bawah
width = Lebar
height = Tinggi
crop-keys = Area pangkas: panah untuk menggeser, Ctrl+panah untuk mengubah ukuran; dengan Shift, per sepuluh piksel
whole-picture = Seluruh gambar
    .hint = Pakai seluruh gambar
    .exact = Pakai seluruh gambar, atau seluas yang muat dengan skala tetap dari sudut kiri atasnya
middle = Tengah
    .hint = Pakai bagian tengah seluas yang muat dengan skala tetap
aspect-ratio = Rasio aspek
    .hint = Rasio lebar terhadap tinggi yang dijaga area pangkas saat diseret
    .square = TGS harus persegi, dan “{ square }” disetel ke “{ square-off }”, yang mempertahankan bentuk area pangkas: memilih ini kembali memaksanya menjadi persegi
shape-free = Bebas
    .hint = Bentuk apa saja
shape-preserve = Asli
    .hint = Bentuk asli input
shape-custom = Kustom
    .hint = Bentuk yang Anda ketik: sebagai rasio, atau sebagai ukuran hasil
    .pick = Pilih “{ shape-custom }” untuk mengetik bentuk
ratio-result = Hasil
    .fixed = Hasil dijadikan persegi atau punya margin, jadi ukurannya tidak mengikuti area pangkas
    .width = Lebar hasil
    .height = Tinggi hasil
square = Paksa jadi persegi
    .hint = Cara hasil dijadikan persegi
    .tgs = Cara pixel art dijadikan persegi (semua TGS harus persegi)
square-off = Mati
    .hint = Hasil mempertahankan bentuk area pangkas
    .emoji = Emoji harus persegi
    .tgs = TGS harus persegi, begitu pula area pangkas yang dipertahankannya: memilih ini menyetel “{ aspect-ratio }” ke 1:1
    .kept = Pixel art mempertahankan bentuk area pangkas, yang dijaga tetap persegi oleh “{ aspect-ratio }” (semua TGS harus persegi)
square-pad = Tambah margin
    .hint = Margin transparan mengisi sisa persegi
    .tgs = Margin transparan mengisi sisa persegi
square-crop = Pangkas
    .hint = Bagian tengah mengisi persegi dan sisanya dipotong: pratinjau meredupkannya
    .tgs = Bagian tengah mengisi persegi dan sisanya dipotong
square-stretch = Regangkan
    .hint = Gambar diregangkan menjadi persegi
    .tgs = Pixel art diregangkan menjadi persegi (pikselnya tidak lagi persegi)
square-exact = Dengan skala integer yang tetap, seluruh gambar dipertahankan: beri margin saja
scaling = Penskalaan
    .hint = Cara piksel input menjadi piksel hasil
scaling-auto = Otomatis
    .hint = Piksel tajam untuk pixel art yang diperbesar dua kali atau lebih, halus untuk yang lain
scaling-smooth = Halus
    .hint = Piksel yang bersebelahan berbaur
scaling-sharp = Piksel tajam
    .hint = Tiap piksel menjadi blok, dan blok hanya berbaur di tepinya
scaling-integer = Integer
    .hint = Tiap piksel menjadi kelipatan bulat piksel hasil yang sama, atau satu blok utuh piksel menjadi satu; margin transparan mengisi sisanya
integer-scale = Skala integer
    .hint = Berapa piksel hasil yang dibuat dari tiap piksel input, atau berapa piksel input yang menjadi satu
    .only = Hanya dengan penskalaan “{ scaling-integer }”
scale-largest = Terbesar yang muat
    .hint = Kelipatan bulat terbesar yang menjaga area pangkas tetap di dalam hasil
    .preset = Preset menetapkan skalanya
scale-fixed = Tetap
    .hint = Skala ini; area pangkas paling besar sebesar yang muat di hasil
smaller = Lebih kecil
larger = Lebih besar
scale-field = Ketik 3 untuk tiga kali lebih besar, atau 1/3 untuk sepertiganya
    .pick = Pilih “{ scale-fixed }” untuk menentukan skala
scale-unused = Skala integer hanya berlaku dengan penskalaan “{ scaling-integer }”
scale-unsuited = Area pangkas belum cocok dengan skala ini
scale-makes = { $scale } menjadikan area pangkas { $width } × { $height } px
scale-makes-padded = { $scale } menjadikan area pangkas { $width } × { $height } px; margin transparan mengisi sisa { $box_width } × { $box_height }
scale-each-way = ×{ $across } horizontal dan ×{ $down } vertikal
scale-shrinks = Area pangkas lebih besar dari hasil, jadi diperkecil dengan piksel tajam: skala tetap seperti ×1/2 memperkecilnya per blok utuh
margins = Margin
    .hint = Baris dan kolom tepi yang transparan di semua frame input
auto-crop = Pangkas margin kosong otomatis
    .hint = Memangkas pixel art menjadi persegi panjang terkecil yang memuat piksel terlihat di frame mana pun
    .after = Diterapkan setelah pemangkasan di atas
art-pixel = Ukuran piksel art
    .hint = Berapa piksel input untuk satu piksel art. Piksel di luar kisinya dipindahkan ke kisi
art-pixel-detected = Terdeteksi
    .hint = Ditemukan dari pixel art: blok satu warna terkecilnya


## Settings: time

time = Waktu
used-part = Bagian terpakai
    .hint = Awal dan akhir bagian input yang dipakai; linimasa di bawah pratinjau juga mengaturnya
part-from = Dari
part-to = sampai
part-first = Frame pertama
part-last = Frame terakhir
all-of-input = Seluruh input
part-seconds = { $from } sampai { $to } dari { $length }
part-frame = Frame { $number } dari { $count }
part-frames = Frame { $first } sampai { $last } dari { $count }
speed = Kecepatan
    .hint = Berapa kali lebih cepat hasil diputar dibanding input
from-preset = Ikut preset
    .hint = Nilai yang diatur preset
from-input = Ikut input
    .speed = Secepat input diputar
    .rate = Laju input pada kecepatan ini, maksimal 60 fps
fits-3-s = Muat 3 s
    .hint = Secepat yang diperlukan agar bagian terpakai muat dalam 3 detik
fit-3-s = Jadikan 3 s
    .hint = Atur kecepatan agar bagian terpakai diputar dalam 3 detik
plays = Diputar { $played }
plays-cut = Diputar { $kept } dari { $used }: sisanya dipotong
plays-spoofed = Diputar penuh { $played }: Telegram diberi tahu durasinya lebih pendek
frame-rate = Frame rate
    .hint = Lebih sedikit frame per detik menyisakan lebih banyak byte untuk tiap frame; lebih dari laju input hanya akan mengulang frame
    .sequence = Seberapa cepat frame dalam urutan ini diputar
    .sheet = Seberapa cepat frame dalam urutan atau lembar sprite diputar
default-rate = Default
    .hint = { $fps } fps, karena frame ini tidak menyebutkan kecepatan putarnya
over-3-s = Palsukan durasi
    .hint = Telegram menolak stiker yang menyatakan durasinya lebih dari 3 detik
spoof-auto = Jika lebih panjang
    .hint = Stiker yang lebih dari 3 detik diberi durasi pendek di dalamnya: Telegram menerimanya dan memutar semuanya
spoof-always = Selalu
    .hint = Stiker pendek pun diberi durasi pendek di dalamnya
spoof-never = Tidak: potong di 3 s
    .hint = Tidak memalsukan metadata dan memotong video di 3 detik
spoof-to = Durasi palsu
    .hint = Durasi yang ditulis di metadata, yang dibaca Telegram
    .honest = Tidak ada yang dipalsukan jika “{ spoof-never }” dipilih
default-duration = Default
    .hint = Durasi pendek yang diterima Telegram
frame = Frame
frame-at = Frame pada
    .hint = Waktu frame yang dijadikan gambar; linimasa di bawah pratinjau juga memilihnya


## Settings: size and quality of a WebM

size-and-quality = Ukuran dan kualitas
fitting = Penyesuaian ukuran
    .hint = Apa yang diubah tgradish agar ukuran file tepat di bawah batas
    .lossless = Enkode lossless tidak punya laju bit atau kualitas untuk disesuaikan: memilih ini menghapus centang “{ lossless-encoding }”
fit-auto = Tampilan terbaik
    .hint = Menyesuaikan laju bit; jika tidak ada yang muat, atau jika frame yang lebih sedikit (masing-masing dengan lebih banyak bit) bisa terlihat lebih baik, mencoba frame rate lebih rendah dan menyimpan yang paling mirip input
fit-bitrate = Laju bit
    .hint = Menyesuaikan laju bit pada frame rate yang dipilih
fit-crf = Kualitas
    .hint = Menyesuaikan nilai kualitas konstan (CRF)
fit-fps = Frame rate
    .hint = Menyesuaikan frame rate pada kualitas konstan
fit-length = Panjang
    .hint = Menyesuaikan panjang pada kualitas konstan, dengan memotong bagian akhir
fit-off = Mati
    .hint = Mengenkode sekali dengan nilai di bawah, meski hasilnya terlalu besar
encoder = Upaya enkoder
    .hint = Enkode yang lebih cepat terlihat lebih buruk pada ukuran yang sama
encoder-fast = Cepat
    .hint = Hitungan detik, tapi jelas lebih buruk pada ukuran yang sama
encoder-balanced = Seimbang
encoder-best = Terbaik
    .hint = Lambat: butuh beberapa menit untuk video panjang
fitting-off = “{ fit-off }” mengenkode sekali
attempts = Enkode hingga
    .hint = Paling banyak berapa kali penyesuaian boleh mengenkode
attempts-default = Default
    .hint = Cukup untuk sampai beberapa persen di bawah batas ukuran file bagi sebagian besar input
attempts-times = kali
search-within = Cari dalam
    .hint = Rentang yang dicari penyesuaian, dalam satuan nilai yang disesuaikan
    .low = Terendah
    .high = Tertinggi
search-auto = Otomatis
    .hint = Selebar yang sesuai dengan cara penyesuaian dan inputnya
range-to = sampai
crf = Kualitas (CRF)
    .hint = Kualitas konstan dari 0, yang terbaik, sampai 63
    .lossless = Lossless mengenkode tanpa nilai kualitas
    .fitted = Penyesuaian mencari kualitasnya
    .bitrate = Penyesuaian mencari laju bit, bukan kualitas
crf-default = Default
    .hint = Kualitas yang baik untuk ukuran stiker
lossless = Lossless
lossless-encoding = Enkode lossless
    .hint = Ukurannya kecil hanya untuk gambar mungil atau diam
    .switches = Ukurannya kecil hanya untuk gambar mungil atau diam. Enkode lossless tidak punya laju bit atau kualitas untuk disesuaikan: mencentang ini mengubah penyesuaian ke “{ fit-fps }”
bitrate = Laju bit
    .hint = Dipakai kecuali hanya kualitas yang diatur
    .fitted = Penyesuaian mencari laju bitnya
    .constant = Dienkode pada kualitas konstan, bukan laju bit
bitrate-auto = Dari batas ukuran file
    .hint = Sebanyak yang muat untuk panjangnya


## Settings: size and quality of a TGS animation

tgs-encoder = Upaya enkoder
    .hint = Seberapa keras mencari enkode yang kecil
tgs-fast = Cepat
    .hint = Kurang dari satu detik, beberapa persen lebih besar
tgs-balanced = Seimbang
tgs-best = Terbaik
    .hint = Beberapa detik, file terkecil
too-large = Jika tidak muat
    .hint = Apa yang terjadi jika pixel art apa adanya tidak muat dalam 64 KiB
never-change = Jangan ubah pixel art; laporkan saja
    .set = Pixel art tidak pernah diubah, sesuai pengaturan di atas
give-up = Korbankan dulu
    .hint = Jika pixel art harus diubah agar muat: geraknya atau detailnya
give-up-either = Mana saja
    .hint = Mana pun yang paling sedikit mengubah pixel art untuk byte yang dihemat
give-up-motion = Gerak
    .hint = Frame digabung dan dibuang dulu: detail penuh, gerak lebih patah-patah
give-up-detail = Detail
    .hint = Gambar dibuat lebih kasar dulu: gerak mulus, detail berkurang
ways-to-fit = Cara agar muat
    .hint = Apa yang boleh dilakukan penyesuaian pada pixel art agar muat dalam 64 KiB, mulai dari yang paling tidak terlihat
reduce-snap = Selaraskan ke kisi piksel
    .hint = Memindahkan piksel di luar kisi ke kisi piksel art
reduce-colours = Gabung warna yang mirip
    .hint = Warna yang hampir sama menjadi satu
reduce-specks = Hapus bintik
    .hint = Piksel tunggal yang berbeda dari semua piksel di sekitarnya
reduce-frames = Gabung frame yang mirip
    .hint = Frame yang hampir sama menampilkan satu gambar
reduce-drop = Buang frame
    .hint = Menampilkan selang satu frame, atau lebih jarang
reduce-resolution = Turunkan resolusi
    .hint = Piksel art yang lebih sedikit dan lebih besar
reading-art = Pembacaan pixel art
sprite-sheet = Lembar sprite
    .hint = Frame yang disusun dalam kisi sel berukuran sama, baris demi baris
read-as-sheet = Baca sebagai lembar sprite
sheet-refused = File ini sudah punya { $frames } frame: lembar sprite adalah satu gambar
    .converted = Dibaca apa adanya, bukan sebagai lembar sprite: file ini sudah punya { $frames } frame
sheet-cells = Sel
    .columns = Kolom
    .rows = Baris
sheet-frames = Frame
sheet-all = Semua
    .hint = Setiap sel kecuali sel transparan di akhir


## Settings: an Aseprite file's tags and layers

aseprite-file = File Aseprite
unreadable-choices = Tidak bisa dibaca dengan tag dan lapisan yang dipilih
read-all = Baca semuanya
tag = Tag
    .hint = Memilih frame sebuah tag: sesuai awal dan akhirnya, atau dibaca dengan urutannya sendiri jika diputar mundur atau bolak-balik
all-frames = Semua frame
    .hint = Setiap frame file
tag-frame = Frame { $number }
tag-frames = Frame { $from } sampai { $to }
tag-reverse = { $frames }, mundur
tag-ping-pong = { $frames }, maju lalu mundur
tag-pong-ping = { $frames }, mundur lalu maju
layers = Lapisan
    .hint = Lapisan yang ditampilkan, yang teratas lebih dulu; grup yang disembunyikan menyembunyikan lapisan di dalamnya
    .group-hidden = Grupnya disembunyikan
    .file = Kembali ke lapisan yang ditampilkan file


## Settings: encoder options, folded away under the rest

encoder-options = Opsi enkoder
vp9-options = Opsi enkoder berdasarkan nama, misalnya tune-content = screen untuk grafis datar. ffmpeg -h encoder=libvpx-vp9 menampilkan daftarnya
    .name = Contoh: tune-content
    .value = Contoh: screen
add-option = Tambah opsi
examples = Contoh:
example-tune-content = Grafis datar dan rekaman layar: tepi lebih tajam
example-aq-mode = Memakai lebih banyak bit di bagian gambar yang rumit
example-sharpness = Mempertahankan lebih banyak detail halus, dengan sedikit mengorbankan kemulusan
example-arnr-strength = Pengurangan derau antar-frame lebih sedikit
example-g = Keyframe setidaknya setiap 60 frame
example-qmax = Tidak pernah lebih buruk dari kualitas 50 di bagian mana pun
ffmpeg-arguments = Argumen ffmpeg
    .hint = Argumen mentah yang ditambahkan sebelum output, dipisah seperti oleh shell
    .placeholder = Contoh: -qmin 4
    .unfinished = Ada tanda kutip atau garis miring terbalik yang belum ditutup: argumen tetap seperti terakhir dibaca sampai ditutup
    .system = Hanya dengan ffmpeg sistem, yang bisa dipilih di “{ settings }”


## What keeps a file from being converted as it is, and the ways out

blocked = Belum bisa dikonversi: { $reason }
    .heading = Belum bisa dikonversi
blocked-crop = area pangkas berada di luar gambar
blocked-sheet = sel lembar sprite tidak membagi gambar dengan pas
blocked-tag = file tidak punya tag “{ $tag }”
blocked-layers = file tidak punya lapisan { $layers }
blocked-ffmpeg = ffmpeg tidak ditemukan
crop-outside = Area pangkas, { $width } × { $height } dari { $left }, { $top }, berada di luar gambar { $across } × { $down } ini
use-whole = Pakai seluruh gambar
    .hint = Tanpa pangkas, mempertahankan seluruh gambar
sheet-uneven = Gambar { $width } × { $height } tidak terbagi menjadi { $columns } × { $rows } sel yang sama besar
sheet-nearest = Terdekat yang pas: { $columns } × { $rows }
    .hint = Susunan terdekat dengan ini yang selnya membagi gambar dengan pas
tag-missing = File ini tidak punya tag “{ $tag }”
layers-missing = File ini tidak punya lapisan { $layers }
layers-as-file = Tampilkan lapisan seperti di file
    .hint = Tampilkan dan sembunyikan lapisan seperti di file
needs-ffmpeg = Ini butuh ffmpeg, yang tidak ditemukan
    .hint = Pasang ffmpeg dari paket sistem Anda atau dari ffmpeg.org, atau pilih lokasinya. File dibaca ulang setelah ffmpeg ditemukan
choose-ffmpeg = Pilih ffmpeg…
    .hint = Pilih program ffmpeg, atau folder berisi ffmpeg dan ffprobe
row-attention = perlu perhatian
    .hint = Ada yang mencegahnya dikonversi: lihat pengaturannya
row-needs-ffmpeg = butuh ffmpeg
    .hint = ffmpeg diperlukan untuk membaca file ini atau membuat hasilnya, tetapi tidak ditemukan
need-attention = { $count } perlu perhatian


## What converting will do, beside the preview, step by step

plan-video = Video akan:
plan-picture = Gambar akan:
plan-animation = Animasi akan:
plan-art = Pixel art akan:
plan-video-frame = Satu frame video akan:
plan-animation-frame = Satu frame animasi akan:
plan-art-frame = Satu frame pixel art akan:
plan-tag = Diputar sebagai tag “{ $tag }”
plan-tag-reverse = Diputar sebagai tag “{ $tag }”, mundur
plan-tag-ping-pong = Diputar sebagai tag “{ $tag }”, maju lalu mundur
plan-tag-pong-ping = Diputar sebagai tag “{ $tag }”, mundur lalu maju
plan-cropped = Dipangkas menjadi { $width } × { $height }
plan-frame = Diambil pada { $time }
plan-scaled = Diskalakan menjadi { $width } × { $height }
plan-stretched-across = Diregangkan ×{ $factor } horizontal
plan-stretched-down = Diregangkan ×{ $factor } vertikal
plan-middle-cut = Bagian tengah dipotong menjadi { $width } × { $height }
plan-padded = Diberi margin menjadi { $width } × { $height }
plan-trimmed = Dipangkas ke piksel yang terlihat
plan-grid = Piksel diselaraskan ke kisi { $scale } × { $scale }
plan-square-crop = Dipangkas menjadi persegi
plan-square-stretch = Diregangkan menjadi persegi
plan-square-pad = Diberi margin menjadi persegi
plan-cut = Dipotong ke { $from }–{ $to } dari { $length }
plan-faster = Dipercepat ×{ $factor }
plan-slower = Diperlambat ×{ $factor }
plan-cut-3-s = Dipotong di 3 s dari { $played }
plan-spoofed = Durasi dipalsukan, jadi seluruh { $played } diputar
plan-spoofed-anyway = Durasi dipalsukan, meski { $played } sudah cukup pendek
plan-saved = Disimpan sebagai { $name }


## Settings: the same conversion as commands, folded away under the rest

commands = Perintah
copy = Salin
commands-posix = Konversi yang sama di baris perintah, untuk shell POSIX:
commands-powershell = Konversi yang sama di baris perintah, untuk PowerShell:
pasted-temporary = Gambar yang ditempel disimpan di folder sementara, yang hilang saat tgradish ditutup
ffmpeg-commands = Perintah ffmpeg untuk enkode pertama; penyesuaian mengubah laju bit atau kualitasnya, dan mungkin frame rate, di antara enkode:
    .builtin = Yang dilakukan ffmpeg bawaan untuk enkode pertama, sebagai perintah untuk ffmpeg sistem; penyesuaian mengubah laju bit atau kualitasnya, dan mungkin frame rate, di antara enkode:
no-ffmpeg-commands = Tidak ada perintah ffmpeg: { $why }
    .unread = input belum dibaca
    .art = ffmpeg membaca PNG yang digambar dari pixel art di setiap konversi


## The Settings dialog

save = Simpan
saved-to = Disimpan ke { $path }
    .kept = Disimpan ke { $path }; file yang tidak dapat dibaca disimpan sebagai { $kept }
not-saved = Tidak dapat menyimpan: { $error }
no-home = Tidak ada folder home untuk menyimpan pengaturan
settings-note = Perubahan langsung diterapkan dan disimpan
reset-settings = Pulihkan default
    .hint = Kembalikan setiap pengaturan di sini seperti awalnya di tgradish
ffmpeg-given = Kali ini tgradish memakai ffmpeg yang diberikan saat dijalankan; pengaturan ini berlaku jika dijalankan tanpanya
ffmpeg-use = Pakai
    .hint = Video dibaca, dan stiker WebM dibuat, dengan ffmpeg
ffmpeg-auto = Otomatis
    .hint = ffmpeg bawaan jika build ini memilikinya, jika tidak ffmpeg sistem
ffmpeg-builtin = Bawaan
    .hint = ffmpeg di dalam tgradish
    .missing = Build tgradish ini tidak menyertakan ffmpeg
ffmpeg-system = Sistem
    .hint = ffmpeg dan ffprobe di PATH; memungkinkan argumen ffmpeg mentah
ffmpeg-program = Program
    .hint = File eksekutabel ffmpeg, atau folder berisi ffmpeg dan ffprobe; mengesampingkan pilihan di atas
    .missing = Tidak ada apa pun di sana, jadi program yang dipakai tetap seperti sebelumnya
ffmpeg-found = { $path }, ditemukan di PATH
for-example = Contoh: { $example }
choose = Pilih…
check = Periksa
    .hint = Periksa apakah ffmpeg ini bisa membuat stiker
checking-ffmpeg = Memeriksa…
ffmpeg-ready = ffmpeg { $version } dengan VP9: siap
ffmpeg-no-vp9 = ffmpeg { $version } tidak bisa mengenkode VP9 dengan libvpx
converting-settings = Konversi
threads = Thread
    .hint = Jumlah maksimum yang dipakai konversi sekaligus; lebih sedikit menyisakan lebih banyak daya komputer untuk yang lain, tapi lebih lama
threads-auto = Otomatis
    .hint = Sebanyak yang bisa dimanfaatkan tiap tahap konversi
results = Hasil
save-results = Simpan
next-to-input = Di sebelah tiap input
in-a-folder = Di folder
    .unchosen = Sampai folder dipilih, hasil disimpan di sebelah tiap input
    .relative = Tulis jalur lengkap (absolut), agar folder yang dipakai bisa berubah
    .file = Itu file, bukan folder, jadi folder yang dipakai tetap seperti sebelumnya
existing-files = File yang ada
    .hint = Hasil buatan tgradish selalu diganti saat dibuat ulang
replace-in-the-way = Ganti file apa pun yang menghalangi
interface = Antarmuka
language = Bahasa
    .hint = Bahasa yang ditampilkan di sini; baris perintah tetap berbahasa Inggris
    .variable = Dipilih oleh variabel lingkungan { $variable }
as-the-system = Ikut sistem
as-the-system-language = Ikut sistem: { $language }
theme = Tema
    .hint = Tombol di samping ⚙ juga beralih antara terang dan gelap
theme-light = Terang
theme-dark = Gelap
accent = Warna aksen
    .hint = Menandai yang dipilih, yang disorot, dan yang difokuskan
scale = Skala
    .hint = Seberapa besar teks dan kontrol
    .keys = { $larger } dan { $smaller } juga mengubahnya; { $reset } kembali ke 100%
scale-smaller = Lebih kecil
scale-larger = Lebih besar
scale-reset = Kembali ke 100%
scale-percent = { $percent }%
accent-system = { as-the-system }
    .hint = Warna aksen desktop, atau biru Breeze jika tidak ada
accent-pink = Merah muda
accent-red = Merah
accent-orange = Oranye
accent-yellow = Kuning
accent-green = Hijau
accent-teal = Toska
accent-blue = Biru
accent-purple = Ungu
accent-violet = Violet
accent-grey = Abu-abu
accent-own = Warna pilihan sendiri
    .chosen = Warna pilihan sendiri, { $colour }
accent-shade = Nuansa
    .value = Saturasi { $saturation }%, kecerahan { $value }%
accent-hue = Rona
accent-hex = Hex
    .wrong = Tulis sebagai # dan enam digit heksadesimal, seperti #3daee9
files = File
reopen-files = Buka lagi file dari sesi terakhir, beserta pengaturannya
motion = Gerakan
    .hint = Jika sistem meminta gerakan dikurangi, tidak ada yang bergerak apa pun pilihannya
motion-on = Gulir dengan halus, serta efek pudar dan geser saat tampilan berubah
new-files = File baru
preset-for-webm = Preset untuk WebM
preset-for-tgs = Preset untuk TGS


## The result, beside the preview

last-conversion = Konversi terakhir
path-in-the-way = { $path } menghalangi
stopped-early = Dihentikan sebelum selesai
result = Hasil
    .stale = Hasil, usang
result-copy = Salin ({ $keys }) untuk menempelkannya ke obrolan Telegram
result-webm = { $width } × { $height } px, { $fps } fps, { $length }
    .spoofed = { $width } × { $height } px, { $fps } fps, { $length }, durasi dipalsukan
changed-to-fit = Diubah agar muat: rinciannya di bawah
replaced-earlier = Mengganti hasil sebelumnya yang dibuat tgradish
converting = Mengonversi
stage-reading = Membaca pixel art
stage-drawing = Mengubah ke vektor
stage-fitting-steps = Menyesuaikan ke 64 KiB: { $count } langkah, sekitar { $size }
stage-fitting-from = Menyesuaikan ke 64 KiB dari { $size }
stage-compressing = Mengompres
    .hopeless = Mengompres cepat: tidak akan muat
    .missed = Dikompres → { $size }, terlalu besar
    .missed-far = Mengompres: masih jauh melebihi batas
retry = Percobaan { $number }: target { $aim }
retry-done = Percobaan { $number }: target { $aim } → { $size }, terlalu besar
    .far = Percobaan { $number }: target { $aim } → masih jauh melebihi batas
starting = Memulai
encode = Enkode { $number }: { $fps } fps, { $rate }
encode-of = Enkode { $number } dari maks. { $most }: { $fps } fps, { $rate }
encode-done = Enkode { $number }: { $fps } fps, { $rate } → { $size }, muat
    .too-large = Enkode { $number }: { $fps } fps, { $rate } → { $size }, terlalu besar
rate-quality = kualitas { $crf }
rate-lossless = lossless
encodes-up-to = Hingga { $count } enkode, sampai ada yang tepat di bawah batas ukuran file
art-size = { $width } × { $height } piksel art
    .scaled = { $width } × { $height } piksel art, masing-masing { $scale } × { $scale } piksel input
art-summary = { $size }; { $colours }, { $frames }
art-tempo = Diputar ×{ $tempo } kecepatan input
art-off-grid = Sebagian besar terlihat seperti pixel art { $scale }× dengan beberapa piksel di luar kisi; ukuran piksel art { $scale } akan menyelaraskannya
art-shapes = { $layers } berisi { $rectangles }, { $size } tanpa kompresi
art-before-fitting = Sekitar { $size } apa adanya; diubah agar muat:
art-step = { $change }, sekitar { $size } setelahnya
step-snapped = piksel diselaraskan ke kisi { $scale }
step-colours = warna dengan selisih di bawah { $distance }% digabung
step-frames = frame yang berubah di bawah { $changed }% digabung
step-dropped = { $share }% frame dibuang
step-specks = piksel tunggal yang menyimpang dihapus
step-resolution = resolusi diturunkan ke { $factor }%
column-number = Setiap enkode secara berurutan; ✔ yang disimpan
column-fps = Frame per detik
column-rate = Laju bit, atau kualitas saat dienkode
column-size = ukuran
    .hint = Porsi dari batas ukuran file, hijau jika muat
column-alike = mirip
    .hint = Seberapa mirip dengan input, diukur dengan SSIM (structural similarity)
rate-quality-short = q { $crf }
decided-in-band = Berhenti: enkode { $number } memakai { $share } dari batas, setidaknya { $least } yang ditargetkan
decided-alike-enough = Berhenti: enkode { $number } sudah { $alike } mirip dengan input, jadi bit tambahan hanya akan memperbesarnya
decided-alike-unchanged = Berhenti: enkode { $number } { $alike } mirip dengan input, hampir sama dengan enkode { $than } yang memakai bit lebih sedikit
decided-little-to-gain = Berhenti: enkode { $number } memakai { $share } dari batas, dan enkode berikutnya hanya akan memakai sekitar { $predicted }
decided-too-large-at-least = Berhenti: bahkan nilai terendah rentang, di enkode { $number }, memakai { $share } dari batas
decided-nothing-left = Berhenti: tidak ada pengaturan lain yang layak dicoba
decided-out-of-attempts = Berhenti: batas { $count } enkode sudah tercapai
decided-lowers-to-fit = Mencoba { $fps } fps: tidak ada yang muat pada { $from } fps
decided-lowers-for-looks = Mencoba { $fps } fps: enkode { $number } pada { $from } fps hanya { $alike } mirip dengan input
decided-settled = Berhenti lebih awal: enkode { $number } disimpan
decided-lower-no-better = Berhenti: { $fps } fps { $alike } mirip dengan input, tidak lebih tinggi dari { $kept_alike } pada enkode { $kept } ({ $kept_fps } fps)
issue-not-webm = Kontainernya { $container }, bukan WebM
issue-no-video = Tidak punya trek video
issue-not-vp9 = Kodeknya { $codec }, bukan VP9
issue-audio = Punya trek audio
issue-size = Ukurannya { $width } × { $height } px, sedangkan Telegram menerima { $side } × { $side } px
    .sticker = Ukurannya { $width } × { $height } px, sedangkan Telegram menerima satu sisi { $side } px dan sisi lainnya tidak lebih dari itu
issue-too-big = Ukurannya { $size }, melebihi batas { $limit }
issue-too-long = Metadatanya menyatakan durasi { $seconds }, melebihi batas { $limit }
issue-too-many-fps = Frame rate-nya { $fps } fps, melebihi batas { $limit } fps
issue-animated = Ini beranimasi, padahal stiker statis hanya satu gambar
issue-frame-rate = Frame rate-nya { $fps } fps, sedangkan Telegram menerima { $required } fps
    .accepted = Frame rate-nya { $fps } fps: melanggar aturan Telegram, meski Telegram menerimanya
issue-no-frames = Tidak ada frame di dalamnya
issue-unpacked = Tanpa kompresi, ukurannya { $size }, melebihi batas { $limit } Telegram Desktop
issue-count =
    .layers = Lapisan: { $count }, melebihi batas { $limit }
    .shapes-per-layer = Bentuk terbanyak dalam satu lapisan: { $count }, melebihi batas { $limit }
    .paint-points = Titik jalur terbanyak dalam satu isian: { $count }, melebihi batas { $limit }
    .paints-per-layer = Isian dan goresan terbanyak dalam satu lapisan: { $count }, melebihi batas { $limit } di aplikasi Telegram
    .paint-sources = Bentuk yang diwarnai terbanyak dalam satu lapisan: { $count }, melebihi batas { $limit } di aplikasi Telegram
    .path-points = Titik terbanyak dalam satu jalur: { $count }, melebihi batas { $limit } di aplikasi Telegram
    .keyframes = Keyframe terbanyak pada satu properti: { $count }, melebihi batas { $limit } di aplikasi Telegram
    .assets = Aset: { $count }, melebihi batas { $limit } di aplikasi Telegram
    .expanded-layers = Lapisan setelah precomp dijabarkan: { $count }, melebihi batas { $limit } di aplikasi Telegram
    .nesting = Tingkat nesting dalam JSON-nya: { $count }, melebihi batas { $limit } di aplikasi Telegram
    .group-depth = Tingkat grup di dalam grup: { $count }, melebihi batas { $limit } di aplikasi Telegram
issue-cost = Beban render: { $cost }, melebihi batas { $limit }; bentuk: { $shapes }, lapisan: { $layers }, masing-masing { $each }
issue-empty-tangents = Jalur dengan tangen kosong ([]), yang ditolak Telegram: { $count }; [0,0] berfungsi
issue-path-coordinate = Koordinat jalur terbesar: { $coordinate }, melebihi batas { $limit } di aplikasi Telegram
issue-precomp-loop = Precomp-nya saling merujuk secara melingkar
issue-feature = { $feature }: melanggar aturan Telegram, meski Telegram menerimanya
    .undrawn = { $feature }: melanggar aturan Telegram; Telegram menerimanya, tetapi aplikasinya tidak menggambarnya
    .ignored = { $feature }: aplikasi Telegram tidak menggambarnya
warned-fewer-frames = Dibaca pada { $fps } fps, karena lebih banyak frame { $width } × { $height } tidak akan muat di memori
warned-many-colours = Ada { $colours }, tidak biasa untuk pixel art, jadi stikernya akan besar
warned-length-capped = Durasi yang dicoba penyesuaian dibatasi hingga { $length }, karena penyesuaian hanya memendekkan
warned-input-shorter = Input hanya berdurasi { $length } setelah titik awalnya: semuanya dipakai
warned-trimmed = Dipotong menjadi { $length }: pemalsuan nonaktif, dan Telegram menolak video yang lebih panjang
warned-left-out = Ditulis tanpa nama tgradish di metadatanya, karena argumen ffmpeg mentah tidak menyisakan ruang untuknya: { $details }
    .duration = Ditulis tanpa durasi palsu di metadatanya, karena argumen ffmpeg mentah tidak menyisakan ruang untuknya: { $details }


## Inspecting a sticker

inspect-another = Periksa lainnya…
inspect-webm-sticker = Stiker video
inspect-webm-emoji = Emoji video
inspect-image-sticker = Stiker statis
inspect-image-emoji = Emoji statis
inspect-tgs = Animasi vektor, { $width } × { $height } pada { $fps } fps
inspect-unknown = Jenis stiker yang belum bisa ditampilkan versi ini
inspect-size = Ukuran
    .hint = Telegram menolak file yang lebih besar
    .tgs = Telegram menolak stiker yang lebih besar
inspect-duration = Durasi
    .hint = Telegram membaca durasi di metadata; pemalsuan membuatnya pendek
    .spoofed = { $seconds } s di metadata, dipalsukan
    .no-header = { $seconds }, tidak ada di metadata
inspect-plays = Diputar
    .hint = Panjang video yang sebenarnya
inspect-frame-rate = Frame rate
    .value = { $fps } fps dari { $most }, { $frames }
inspect-picture = Gambar
    .none = tidak ada
inspect-video = { $width } × { $height } px, { $codec }
    .alpha = { $width } × { $height } px, { $codec }, dengan transparansi
inspect-webp = { $width } × { $height } px, WebP, lossless
    .alpha = { $width } × { $height } px, WebP, lossless, dengan transparansi
    .lossy = { $width } × { $height } px, WebP, lossy
    .lossy-alpha = { $width } × { $height } px, WebP, lossy, dengan transparansi
inspect-audio = Audio
inspect-title = Judul
inspect-made-with = Dibuat dengan
    .hint = Program yang menulis file ini, menurut file itu sendiri
    .image = Program yang disebut di metadatanya
    .unknown = tidak diketahui
inspect-note = Catatan
    .hint = Teks di padding file, yang dilewati pemutar
inspect-name = Nama
    .hint = Nama animasi itu sendiri, yang tidak ditampilkan Telegram
inspect-length = Panjang
    .hint = Stiker animasi paling lama 3 detik
    .value = { $seconds } dari 3.00 s, { $frames }
inspect-cost = Beban render
    .hint = Bentuk ditambah 9 untuk tiap lapisan, menurut hitungan Telegram
inspect-layers = Lapisan
inspect-shapes = Maks. bentuk per lapisan
inspect-refuses-more = Telegram menolak lebih dari ini
inspect-points = Maks. titik per isian
    .hint = Titik jalur yang diwarnai satu isian; Telegram menolak lebih dari ini
inspect-unpacked = Tanpa kompresi
    .hint = Telegram Desktop tidak memutar animasi yang lebih besar
inspect-features = Fitur
    .hint = Fitur Lottie di luar bentuk dan transformasi


## What went wrong, as converting or reading a file reports it

crashed = tgradish mengalami crash: { $details }
ffmpeg-not-found = ffmpeg tidak ditemukan: { $details }
ffmpeg-not-on-path = ffmpeg dan ffprobe tidak ada di PATH: pasang keduanya, atau pilih ffmpeg lain di “{ settings }”
ffmpeg-args-need-program = Argumen ffmpeg butuh ffmpeg sistem, yang bisa dipilih di “{ settings }”; opsi enkoder berfungsi dengan keduanya
program-failed = { $program } gagal ({ $status }):
unreadable-file = Tidak dapat membaca { $name }: { $details }
no-video = { $name } tidak berisi video
no-such-file = { $name } sudah tidak ada
not-webm = { $name } bukan file WebM
not-sticker = { $name } bukan stiker: stiker berupa file WebM, WebP, atau .tgs
no-frame-at = { $name } tidak punya frame pada { $seconds }
nothing-fits = Tidak ada yang muat dalam batas { $limit }: enkode terkecil { $smallest }. Bagian terpakai yang lebih pendek, frame rate lebih rendah, atau rentang pencarian yang lebih lebar bisa membantu
clipboard-failed = Tidak dapat menempel dari papan klip: { $details }
output-is-input = Hasil akan menimpa { $name }, salah satu inputnya
nothing-to-convert = Tidak ada gambar untuk dikonversi


## Saving and managing presets

save-preset-title = { -save-settings }
save-preset-note = Pengaturan file ini, sebagai titik awal untuk file mana pun, kecuali yang hanya cocok untuk file ini: stiker atau emoji, area pangkas, bagian terpakai, tag, lapisan, dan lembar sprite
preset-name = Nama
    .placeholder = stiker saya
preset-description = Deskripsi
    .placeholder = opsional
    .none = tanpa deskripsi
preset-exists = Preset dengan nama ini sudah ada: menyimpan akan menggantinya
replace = Ganti
name-it-first = Beri nama dulu
cancel = Batal
preset-name-length = Nama preset terdiri dari 1 sampai { $most } karakter, tidak diawali spasi
preset-name-characters = Nama preset hanya boleh berisi huruf, angka, spasi, - dan _
preset-name-builtin = “{ $name }” adalah nama preset bawaan
preset-name-taken = Sudah ada preset lain bernama “{ $name }”
preset-extended = Preset “{ $other }” dibuat berdasarkan preset ini: ubah preset itu dulu
your-presets = Preset Anda
your-presets-note = Perubahan langsung disimpan. “{ preset-fast }”, “{ preset-balanced }”, dan “{ preset-best }” adalah bawaan dan tidak berubah
no-presets = Belum ada: “{ -save-settings }”, di bilah di bawah pengaturan
preset-broken = Filenya tidak dapat dibaca
delete-it = Hapus
deleted = “{ $name }” dihapus
undo = Urungkan
    .hint = Kembalikan
not-put-back = Preset “{ $name }” tidak dapat dikembalikan


## About and licences

about-title = Tentang dan lisensi
about-summary = Stiker dan emoji Telegram dari video, gambar, atau pixel art. Lisensi MIT;
source-code = kode sumber
send-feedback = Kirim masukan di Telegram
about-notices = tgradish juga dibangun dari karya orang lain. Lisensi mereka meminta pemberitahuan ini disertakan di setiap salinan, jadi program ini menyertakannya:
notice-licence = { $name }: lisensi
notice-named-licence = { $name }: { $licence }
notice-licence-notes = { $name }: catatan lisensi
notice-patent-grant = { $name }: pemberian paten
notice-sources = { $name }: yang disertakan beserta sumbernya
notice-font = Font: { $name }, subset
notice-crates = Crate Rust
notice-crates-licence = { $licence }: { $crates }
    .more = { $licence }: { $crates }, dan { $more } lainnya
ffmpeg-sources = Kode sumber persis ffmpeg bawaan, untuk rilis ini


## The dialog of keyboard shortcuts, opened from the menu or by ? or F1

key-ctrl = Ctrl
key-cmd = Cmd
key-alt = Alt
key-shift = Shift
key-enter = Enter
key-space = Spasi
key-delete = Delete
key-backspace = Backspace
key-home = Home
key-end = End
key-escape = Esc
key-left = Kiri
key-right = Kanan
key-up = Atas
key-down = Bawah
shortcuts-files = File
shortcuts-playing = Pratinjau dan linimasanya
shortcuts-general = Umum
shortcuts-mouse = Mouse, di pratinjau dan linimasa
shortcut-add-files = Tambah file
shortcut-paste = Tambahkan file atau gambar dari papan klip
shortcut-select = Pilih file di atas atau di bawah
shortcut-convert = Konversi file yang dipilih
shortcut-convert-all = Konversi semua: setiap file yang belum punya hasil terbaru
shortcut-keep-best = Simpan enkode terbaik sejauh ini, saat WebM sedang disesuaikan
shortcut-copy-result = Salin hasil file yang dipilih, untuk ditempel ke Telegram
shortcut-remove = Hapus file yang dipilih, atau yang barisnya sedang difokuskan
shortcut-undo = Urungkan perubahan pada file yang dipilih, atau kembalikan file yang dihapus
shortcut-redo = Ulangi perubahan yang diurungkan
shortcut-play = Putar atau jeda
shortcut-step = Mundur atau maju satu frame; dengan Shift, sepuluh
shortcut-ends = Frame pertama atau terakhir yang dipakai
shortcut-cut-here = Mulai atau akhiri bagian terpakai di frame yang tampil
shortcut-move-start = Geser awal bagian terpakai; dengan Shift, per sepuluh frame
shortcut-move-end = Geser akhir bagian terpakai; dengan Shift, per sepuluh frame
shortcut-settings = Buka Pengaturan
shortcut-inspect = Periksa stiker
shortcut-scale-up = Perbesar semuanya
shortcut-scale-down = Perkecil semuanya
shortcut-scale-reset = Kembali ke 100%
shortcut-shortcuts = Daftar ini
shortcut-close = Tutup dialog atau menu
with-keys = { $hint } ({ $keys })
mouse-drag = Seret
    .does = Pangkas, atau geser area pangkas atau tepinya
mouse-shape = Shift+seret sudut
    .does = Pertahankan bentuk area pangkas, kecuali rasio aspek diatur
mouse-centred = Alt+seret tepi
    .does = Ubah ukuran area pangkas dari tengahnya
mouse-free = Ctrl+seret
    .does = Pangkas tanpa menempel ke tepi dan tengah gambar
mouse-cancel = Esc saat menyeret
    .does = Kembalikan area pangkas seperti semula
mouse-crop-keys = Klik area pangkas, lalu panah
    .does = Geser satu piksel, sepuluh dengan Shift; Ctrl+panah mengubah ukurannya
mouse-double-click = Klik ganda
    .does = Kembali ke seluruh gambar
mouse-wheel = Roda gulir
    .does = Perbesar atau perkecil
mouse-pan = Seret tombol kanan atau tengah
    .does = Geser tampilan, saat diperbesar
mouse-pan-across = Shift+roda gulir
    .does = Geser mendatar, saat diperbesar
mouse-slide = Shift+seret kurung siku
    .does = Geser bagian terpakai di linimasa, dengan panjang tetap
