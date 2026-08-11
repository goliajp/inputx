"""
Find suspect dict rows where 'X(vowel)r(consonant)' pattern in the code,
when expanded to 'X(vowel)er(consonant)', matches another existing row
in library.tsv with the SAME word. This is the 'er → r typo' signature.
"""
import re
from collections import defaultdict

LIB = "core/crates/inputx-pinyin/data/library.tsv"

# Build (code → set of words) and (word → set of codes)
codes_for_word = defaultdict(set)
words_for_code = defaultdict(set)
all_rows = []
with open(LIB, encoding="utf-8") as f:
    for line in f:
        if line.startswith("#") or not line.strip(): continue
        parts = line.rstrip("\n").split("\t")
        if len(parts) < 4: continue
        code, word = parts[0], parts[1]
        codes_for_word[word].add(code)
        words_for_code[code].add(word)
        all_rows.append((code, word, parts[2], parts[3]))

# Find codes that contain "vowel + r + consonant" pattern
VOWELS = set("aiouv")
CONSONANTS = set("bcdfghjklmnpqstwxyz")

suspects = []
for code, word, freq, source in all_rows:
    # Iterate positions where code[i]=vowel, code[i+1]='r', code[i+2]=consonant
    for i in range(len(code)-2):
        if code[i] in VOWELS and code[i+1] == 'r' and code[i+2] in CONSONANTS:
            # Try inserting 'e' between vowel and r
            expanded = code[:i+1] + 'e' + code[i+1:]
            # If the expanded code also exists with the same word → typo signature
            if word in words_for_code.get(expanded, set()):
                suspects.append((code, expanded, word, freq, source))
                break

print(f"er-typo signature rows: {len(suspects)}")
for s in suspects[:20]:
    print(f"  {s[0]:25} → {s[1]:25} word={s[2]:15} freq={s[3]} source={s[4]}")
print(f"... total: {len(suspects)}")
# also save full list
with open("/tmp/er_typos.tsv","w", encoding="utf-8") as f:
    f.write("# typo_code\texpanded_code\tword\tfreq\tsource\n")
    for s in suspects:
        f.write("\t".join(s) + "\n")
