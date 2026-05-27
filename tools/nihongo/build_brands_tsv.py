#!/usr/bin/env python3
"""Generate Japanese brand / chain / SNS / IT-platform entries (TSV).

Output: `tools/scoring/data/supplemental/jp_brands_v1.tsv`. Merged into
jukugo.rs by `build_jukugo_rs.py` (which now reads multiple supplemental
TSVs).

This is the Round-9 wrap-up — closing the most user-visible gap vs Simeji
("typing スタバ for Starbucks just works"). After this round, the JP
plugin is considered "basically on par" with Simeji on the data side;
remaining differences (auto-completion, predictive next-word, user-dict
learning) are engine work, not a data round.

Run from repo root:  python3 tools/jp/build_brands_tsv.py
"""

from __future__ import annotations

from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
OUT = ROOT / "tools/scoring/data/supplemental/jp_brands_v1.tsv"


# Each entry: (kanji_or_kana_form, romaji_reading, freq).
#
# Frequency rubric:
#   95-98  - household names typed every week (sutaba, line, amazon, youtube)
#   85-92  - daily-ubiquitous chains/brands (yunikuro, seven, toyota, sony)
#   75-84  - common but less daily (matsuya, donki, panasonic, tesla)
#   60-74  - long-tail (specialized chains, smaller brands)
#
# Where a brand has multiple common romaji forms (short / long / pun), each
# gets its own row pointing at the same kanji/kana form.
ENTRIES: list[tuple[str, str, int]] = [
    # ---------- カフェ / 食店連鎖 (cafes & restaurant chains) ----------
    ("スタバ", "sutaba", 95),
    ("スターバックス", "sutaabakkusu", 90),
    ("ドトール", "dotouru", 80),
    ("コメダ", "komeda", 78),
    ("コメダ珈琲", "komedakouhi", 72),
    ("タリーズ", "tariizu", 75),
    ("マック", "makku", 92),
    ("マクドナルド", "makudonarudo", 90),
    ("モス", "mosu", 80),
    ("モスバーガー", "mosubaagaa", 78),
    ("ロッテリア", "rotteria", 70),
    ("吉野家", "yoshinoya", 88),
    ("すき家", "sukiya", 88),
    ("松屋", "matsuya", 85),
    ("なか卯", "nakau", 75),
    ("くら寿司", "kurazushi", 80),
    ("スシロー", "sushiroo", 85),
    ("はま寿司", "hamazushi", 78),
    ("回転寿司", "kaitenzushi", 80),
    ("一蘭", "ichiran", 80),
    ("一風堂", "ippuudou", 75),
    ("天下一品", "tenkaippin", 70),
    ("餃子の王将", "gyouzanoouushou", 70),
    ("王将", "ousho", 72),
    ("CoCo壱", "kokoichi", 75),
    ("ココイチ", "kokoichi", 75),
    ("バーミヤン", "baamiyan", 65),
    ("サイゼリヤ", "saizeriya", 75),
    ("ガスト", "gasuto", 78),
    ("デニーズ", "deniizu", 70),
    ("ジョナサン", "jonasan", 65),
    ("びっくりドンキー", "bikkuridonkii", 65),
    ("丸亀製麺", "marugameseimen", 70),
    ("はなまるうどん", "hanamaruudon", 65),
    ("ミスド", "misudo", 80),
    ("ミスタードーナツ", "misutaadoonatsu", 72),
    ("ケンタッキー", "kentakkii", 78),
    ("ケンタ", "kenta", 70),
    ("サブウェイ", "sabuwei", 72),

    # ---------- コンビニ (convenience stores) ----------
    ("セブン", "sebun", 92),
    ("セブンイレブン", "sebun'irebun", 88),
    ("セブンイレブン", "sebunirebun", 88),
    ("ファミマ", "famima", 92),
    ("ファミリーマート", "famiriimaato", 85),
    ("ローソン", "rouson", 90),
    ("ローソンストア100", "rousonsutoaa100", 60),
    ("ミニストップ", "minisutoppu", 70),
    ("デイリーヤマザキ", "deiriiyamazaki", 60),
    ("ポプラ", "popura", 55),

    # ---------- スーパー / ディスカウント ----------
    ("ドンキ", "donki", 88),
    ("ドンキホーテ", "donkihoote", 80),
    ("ドン・キホーテ", "donkihoote", 80),
    ("業務スーパー", "gyoumusuupaa", 72),
    ("業ス", "gyousu", 65),
    ("イオン", "ion", 88),
    ("イトーヨーカドー", "itooyookadoo", 75),
    ("西友", "seiyuu", 75),
    ("ライフ", "raifu", 72),
    ("マルエツ", "maruetsu", 70),
    ("OK", "okay", 65),
    ("オーケー", "ookee", 65),
    ("コストコ", "kosutoko", 80),
    ("ヨドバシ", "yodobashi", 80),
    ("ヨドバシカメラ", "yodobashikamera", 75),
    ("ビックカメラ", "bikkukamera", 78),
    ("ビック", "bikku", 65),
    ("ヤマダ電機", "yamadadenki", 70),
    ("ニトリ", "nitori", 85),
    ("無印", "muji", 88),
    ("無印良品", "mujirushiryouhin", 80),
    ("ロフト", "rofuto", 80),
    ("ハンズ", "hanzu", 70),
    ("東急ハンズ", "toukyuuhanzu", 65),

    # ---------- アパレル / ファッション ----------
    ("ユニクロ", "yunikuro", 95),
    ("GU", "jiiyuu", 85),
    ("ジーユー", "jiiyuu", 80),
    ("しまむら", "shimamura", 78),
    ("ZARA", "zara", 78),
    ("ザラ", "zara", 75),
    ("H&M", "eichiandoemu", 72),
    ("グッチ", "gucchi", 70),
    ("ヴィトン", "viton", 68),
    ("ルイヴィトン", "ruiviton", 72),
    ("シャネル", "shaneru", 70),
    ("プラダ", "purada", 68),
    ("コーチ", "kouchi", 65),
    ("ナイキ", "naiki", 80),
    ("アディダス", "adidasu", 78),
    ("ニューバランス", "nyuubaransu", 70),
    ("コンバース", "konbaasu", 68),
    ("リーバイス", "riibaisu", 65),

    # ---------- 日系メーカー ----------
    ("トヨタ", "toyota", 90),
    ("ホンダ", "honda", 88),
    ("日産", "nissan", 85),
    ("マツダ", "matsuda", 75),
    ("スズキ", "suzuki", 75),
    ("スバル", "subaru", 75),
    ("ダイハツ", "daihatsu", 72),
    ("レクサス", "rekusasu", 75),
    ("ソニー", "sonii", 90),
    ("任天堂", "nintendou", 92),
    ("ニンテンドー", "nintendoo", 88),
    ("Panasonic", "panasonikku", 80),
    ("パナソニック", "panasonikku", 80),
    ("シャープ", "shaapu", 75),
    ("東芝", "toushiba", 78),
    ("日立", "hitachi", 78),
    ("三菱", "mitsubishi", 80),
    ("富士通", "fujitsuu", 75),
    ("NEC", "enuiishii", 72),
    ("キヤノン", "kyanon", 75),
    ("キャノン", "kyanon", 75),
    ("ニコン", "nikon", 70),
    ("オリンパス", "orinpasu", 65),
    ("カシオ", "kashio", 75),
    ("セイコー", "seikou", 70),
    ("カネボウ", "kanebou", 65),
    ("資生堂", "shiseidou", 78),
    ("花王", "kaou", 75),
    ("ライオン", "raion", 72),
    ("味の素", "ajinomoto", 75),
    ("キッコーマン", "kikkooman", 70),
    ("カゴメ", "kagome", 68),
    ("明治", "meiji", 80),
    ("森永", "morinaga", 75),
    ("グリコ", "guriko", 78),
    ("カルビー", "karubii", 78),
    ("江崎グリコ", "ezakiguriko", 65),
    ("サントリー", "santorii", 80),
    ("キリン", "kirin", 80),
    ("アサヒ", "asahi", 80),
    ("サッポロ", "sapporo", 78),
    ("ヤマザキ", "yamazaki", 70),
    ("ヤマザキパン", "yamazakipan", 65),

    # ---------- GAFAM + 外資テック ----------
    ("アマゾン", "amazon", 92),
    ("Amazon", "amazon", 92),
    ("アップル", "appuru", 88),
    ("Apple", "appuru", 85),
    ("グーグル", "guuguru", 88),
    ("Google", "guuguru", 85),
    ("マイクロソフト", "maikurosofuto", 82),
    ("MS", "emuesu", 72),
    ("メタ", "meta", 78),
    ("フェイスブック", "feisubukku", 78),
    ("Facebook", "feisubukku", 75),
    ("テスラ", "tesura", 80),
    ("Tesla", "tesura", 78),
    ("ネットフリックス", "nettofurikkusu", 80),
    ("ネトフリ", "netofuri", 85),
    ("Netflix", "nettofurikkusu", 78),
    ("スポティファイ", "supotifai", 75),
    ("Spotify", "supotifai", 75),
    ("ディズニープラス", "dizuniipurasu", 70),
    ("Adobe", "adobi", 72),
    ("アドビ", "adobi", 72),
    ("Intel", "interu", 70),
    ("インテル", "interu", 70),
    ("NVIDIA", "enbidia", 72),
    ("エヌビディア", "enbidia", 72),
    ("OpenAI", "oupun'ee'ai", 80),
    ("オープンAI", "oupunee'ai", 78),
    ("ChatGPT", "chatto'jiipiitii", 88),
    ("チャットGPT", "chatto'jiipiitii", 85),
    ("Claude", "kuroodo", 75),
    ("クロード", "kuroodo", 75),
    ("Gemini", "jemini", 72),
    ("ジェミニ", "jemini", 72),
    ("Anthropic", "ansoropikku", 65),
    ("アンソロピック", "ansoropikku", 60),

    # ---------- 日本テック ----------
    ("楽天", "rakuten", 88),
    ("メルカリ", "merukari", 90),
    ("ヤフー", "yafuu", 85),
    ("Yahoo", "yafuu", 78),
    ("LINE", "rain", 95),
    ("ライン", "rain", 92),
    ("PayPay", "peipei", 88),
    ("ペイペイ", "peipei", 85),
    ("ドコモ", "dokomo", 85),
    ("au", "eeyuu", 78),
    ("ソフトバンク", "sofutobanku", 82),
    ("ソフバン", "sofuban", 70),
    ("UQ", "yuukyuu", 65),
    ("ワイモバイル", "waimobairu", 65),
    ("LINEMO", "rainmo", 65),
    ("ahamo", "ahamo", 65),
    ("povo", "povo", 60),
    ("DMM", "diiemuemu", 72),
    ("ニコニコ", "nikoniko", 80),
    ("ニコ動", "nikodou", 75),
    ("ニコ生", "nikonama", 70),
    ("ピクシブ", "pikushibu", 75),
    ("pixiv", "pikushibu", 75),
    ("クックパッド", "kukkupaddo", 75),
    ("食べログ", "tabelog", 78),
    ("ホットペッパー", "hottopeppaa", 70),
    ("じゃらん", "jaran", 72),
    ("ぐるなび", "gurunavi", 72),
    ("ZOZO", "zozo", 80),
    ("ゾゾタウン", "zozotaun", 75),
    ("ZOZOTOWN", "zozotaun", 75),
    ("ZOZO TOWN", "zozotaun", 70),
    ("リクルート", "rikuruuto", 75),
    ("サイバーエージェント", "saibaa'eejento", 65),
    ("ガンホー", "ganhoo", 65),
    ("バンダイ", "bandai", 75),
    ("バンナム", "bannamu", 70),
    ("コナミ", "konami", 78),
    ("カプコン", "kapukon", 78),
    ("スクエニ", "sukueni", 75),
    ("スクウェアエニックス", "sukueaenikkusu", 65),
    ("セガ", "sega", 78),
    ("ポケモン", "pokemon", 88),
    ("ピカチュウ", "pikachuu", 80),

    # ---------- SNS / メッセンジャー / 動画 ----------
    ("ツイッター", "tsuittaa", 88),
    ("X", "ekkusu", 80),
    ("Twitter", "tsuittaa", 80),
    ("インスタ", "insuta", 92),
    ("インスタグラム", "insutaguramu", 85),
    ("Instagram", "insutaguramu", 80),
    ("ティックトック", "tikkutokku", 85),
    ("TikTok", "tikkutokku", 80),
    ("ユーチューブ", "yuuchuubu", 90),
    ("YouTube", "yuuchuubu", 88),
    ("ようつべ", "youtsube", 75),
    ("ニコニコ動画", "nikonikodouga", 75),
    ("ディスコード", "disukoodo", 80),
    ("Discord", "disukoodo", 78),
    ("スレッズ", "surezzu", 78),
    ("Threads", "surezzu", 75),
    ("Slack", "surakku", 78),
    ("スラック", "surakku", 75),
    ("Notion", "noushon", 75),
    ("ノーション", "noushon", 72),
    ("ノートン", "nooton", 60),
    ("Reddit", "redditto", 70),
    ("レディット", "redditto", 65),
    ("Bilibili", "biribiri", 65),
    ("ビリビリ", "biribiri", 65),
    ("Weibo", "weibo", 60),
    ("ウィーチャット", "wiichatto", 60),
    ("WhatsApp", "whatsappu", 70),
    ("Telegram", "teregramu", 72),
    ("テレグラム", "teregramu", 70),
    ("Signal", "shigunaru", 65),
    ("カカオトーク", "kakaotouku", 65),
    ("Skype", "sukaipu", 75),
    ("スカイプ", "sukaipu", 72),
    ("Zoom", "zuumu", 85),
    ("ズーム", "zuumu", 80),
    ("Teams", "chiimuzu", 75),
    ("チームス", "chiimuzu", 70),
    ("Meet", "miito", 70),
    ("Webex", "webekkusu", 60),

    # ---------- 通販 / 决済 / 配送 ----------
    ("PayPal", "peipaaru", 70),
    ("Stripe", "sutoraipu", 65),
    ("Square", "sukuea", 65),
    ("Suica", "suika", 88),
    ("スイカ", "suika", 85),
    ("PASMO", "pasumo", 88),
    ("パスモ", "pasumo", 85),
    ("ICOCA", "ikoka", 75),
    ("メルペイ", "merupei", 80),
    ("LINE Pay", "rainpei", 78),
    ("楽天ペイ", "rakutenpei", 78),
    ("ヤマト", "yamato", 78),
    ("ヤマト運輸", "yamato'unyu", 72),
    ("クロネコ", "kuroneko", 75),
    ("クロネコヤマト", "kuronekoyamato", 70),
    ("佐川", "sagawa", 72),
    ("佐川急便", "sagawakyuubin", 65),
    ("日本郵便", "nihon'yuubin", 72),
    ("郵便局", "yuubinkyoku", 80),
    ("郵便", "yuubin", 80),

    # ---------- 交通 ----------
    ("JR", "jeiaaru", 88),
    ("JR東日本", "jeiaaruhigashinihon", 70),
    ("JR西日本", "jeiaarunishinihon", 70),
    ("新幹線", "shinkansen", 90),
    ("ANA", "eienueei", 75),
    ("JAL", "jaru", 75),
    ("スカイマーク", "sukaimaaku", 65),
    ("ピーチ", "piichi", 70),
    ("ジェットスター", "jettosutaa", 65),
    ("メトロ", "metoro", 75),
    ("東京メトロ", "toukyoumetoro", 72),
    ("Uber", "uubaa", 78),
    ("ウーバー", "uubaa", 75),
    ("UberEats", "uubaaiitsu", 80),
    ("ウーバーイーツ", "uubaaiitsu", 78),
    ("出前館", "demaekan", 75),

    # ---------- メディア ----------
    ("NHK", "enueichikee", 88),
    ("フジテレビ", "fujiterebi", 75),
    ("テレ朝", "tereasa", 72),
    ("日テレ", "nichitere", 75),
    ("テレ東", "teretou", 70),
    ("TBS", "teibiiesu", 75),
    ("WOWOW", "wauwau", 65),

    # ---------- 大学 / 機関 (代表的なものだけ) ----------
    ("東大", "toudai", 80),
    ("東京大学", "toukyoudaigaku", 75),
    ("京大", "kyoudai", 75),
    ("京都大学", "kyoutodaigaku", 72),
    ("早稲田", "waseda", 78),
    ("早大", "soudai", 70),
    ("慶応", "keiou", 78),
    ("慶大", "keidai", 65),
    ("阪大", "handai", 70),
    ("東工大", "toukoudai", 65),
    ("MIT", "emu'aitii", 65),
    ("ハーバード", "haabaado", 70),

    # ---------- ファイル / 規格 / 略語 ----------
    ("PDF", "piidiiefu", 85),
    ("Excel", "ekuseru", 85),
    ("エクセル", "ekuseru", 85),
    ("Word", "waado", 80),
    ("ワード", "waado", 80),
    ("PowerPoint", "pawaapointo", 75),
    ("パワポ", "pawapo", 80),
    ("PPT", "piipiitii", 70),
    ("ChatGPT", "chattogeepiitii", 85),
    ("DVD", "diibuidii", 78),
    ("CD", "shiidii", 78),
    ("USB", "yuuesubii", 80),
    ("Wi-Fi", "waifai", 90),
    ("ワイファイ", "waifai", 88),
    ("Bluetooth", "buruutuusu", 75),
    ("ブルートゥース", "buruutuusu", 70),
    ("GPS", "jiipiiesu", 75),
    ("AI", "eeai", 88),
    ("VR", "buiaaru", 75),
    ("AR", "eeaaru", 70),
    ("XR", "ekkusuaaru", 60),
    ("API", "eepiiai", 75),
    ("URL", "yuuaarueru", 78),
    ("HP", "eichipii", 72),
    ("OS", "ooesu", 75),
    ("UI", "yuuai", 70),
    ("UX", "yuuekkusu", 70),
    ("EV", "iibui", 75),

    # ---------- ゲーム / アニメ・漫画レーベル ----------
    ("プレステ", "puresute", 80),
    ("プレイステーション", "pureisuteeshon", 75),
    ("PS4", "piiesufoo", 75),
    ("PS5", "piiesufaibu", 78),
    ("Switch", "suicchi", 80),
    ("スイッチ", "suicchi", 78),
    ("Xbox", "ekkusubokkusu", 72),
    ("エックスボックス", "ekkusubokkusu", 70),
    ("ジブリ", "jiburi", 85),
    ("スタジオジブリ", "sutajiojiburi", 75),
    ("ピクサー", "pikusaa", 75),
    ("ディズニー", "dizunii", 85),
    ("マーベル", "maaberu", 78),
    ("ジャンプ", "janpu", 85),
    ("週刊ジャンプ", "shuukanjanpu", 70),
    ("マガジン", "magajin", 75),
    ("サンデー", "sandee", 70),
    ("アニメイト", "animeito", 75),
    ("メロンブックス", "meronbukkusu", 65),
    ("コミケ", "komike", 78),
    ("コミックマーケット", "komikkumaaketto", 65),
]


def main() -> int:
    seen: set[tuple[str, str]] = set()
    rows: list[tuple[str, str, int]] = []
    for kanji, reading, freq in ENTRIES:
        # Reading must be pure ASCII alpha for the FFI / engine constraints.
        # Skip rows whose romaji somehow includes digits or punctuation —
        # those need to be re-spelled (Wi-Fi, PS4) in the source list above.
        if not reading or not all(c.isalpha() for c in reading):
            continue
        key = (reading, kanji)
        if key in seen:
            continue
        seen.add(key)
        rows.append((kanji, reading, freq))

    rows.sort(key=lambda r: (-r[2], r[1], r[0]))

    OUT.parent.mkdir(parents=True, exist_ok=True)
    with OUT.open("w") as f:
        f.write("# Japanese brand / chain / SNS / IT-platform table — generated by tools/jp/build_brands_tsv.py\n")
        f.write("# Format: form<TAB>romaji<TAB>freq.  Merged into jukugo by build_jukugo_rs.py.\n")
        for kanji, reading, freq in rows:
            f.write(f"{kanji}\t{reading}\t{freq}\n")

    print(f"wrote {OUT} ({len(rows)} entries)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
