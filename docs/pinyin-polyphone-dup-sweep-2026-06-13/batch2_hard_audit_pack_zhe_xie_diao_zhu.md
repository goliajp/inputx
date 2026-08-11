# batch2-hard audit pack · 着 / 血 / 调 / 著

**Purpose**: prep data for the 4 hardest batch2 polyphone-dup chars.
Each char's `(default, exceptions, uncertain)` triple needs native-speaker
audit before applying via `apply_sweep.py`.

**Workflow**: review each char section below; mark final default direction,
trim/extend `draft_keep_*` lists, then I'll write the verified triple into
`batch2_hard_verdict.py` RULES dict and apply.

---

## 字 = `着` (878 rows)

- **Readings**: zhe (助词,看着/随着) · zháo (着火/睡着了/着凉) · zhuó (着重/着陆/附着)
- **pypinyin primary**: zhe (助词)
- **Direction stats**:
  - `zhe` → `zhao` × 439
  - `zhe` → `zhuo` × 439

### Default direction proposal: `del_wrong`

> 助词 zhe 是主流;pypinyin 复制到 zhao/zhuo 的多是机械错(看着/随着/对着/连着 etc 全应保 zhe).

### Exception draft — keep `zhao` reading  *(exception: `del_prim`)*

(26 words)

- `睡着` (not in candidates.tsv)
- `睡不着` (freq=30390)
- `用不着` (freq=26028)
- `找不着` (freq=20341)
- `摸不着` (not in candidates.tsv)
- `用得着` (freq=18714)
- `找得着` (freq=8535)
- `抓得着` (not in candidates.tsv)
- `看得着` (not in candidates.tsv)
- `看不着` (not in candidates.tsv)
- `得着` (not in candidates.tsv)
- `上不着` (not in candidates.tsv)
- `用着` (not in candidates.tsv)
- `买不着` (not in candidates.tsv)
- `买得着` (not in candidates.tsv)
- `着火` (not in candidates.tsv)
- `着凉` (not in candidates.tsv)
- `着急` (not in candidates.tsv)
- `着慌` (not in candidates.tsv)
- `着迷` (not in candidates.tsv)
- `着想` (not in candidates.tsv)
- `着风` (not in candidates.tsv)
- `着了道` (not in candidates.tsv)
- `着了魔` (not in candidates.tsv)
- `捉不着` (not in candidates.tsv)
- `拿不着` (not in candidates.tsv)

### Exception draft — keep `zhuo` reading  *(exception: `del_prim`)*

(21 words)

- `着实` (freq=25786)
- `着重` (not in candidates.tsv)
- `着陆` (not in candidates.tsv)
- `着想` (not in candidates.tsv)
- `着色` (not in candidates.tsv)
- `附着` (not in candidates.tsv)
- `着装` (not in candidates.tsv)
- `沉着` (not in candidates.tsv)
- `着力` (not in candidates.tsv)
- `着手` (not in candidates.tsv)
- `着眼` (not in candidates.tsv)
- `胶着` (not in candidates.tsv)
- `执着` (not in candidates.tsv)
- `穿着` (not in candidates.tsv)
- `着墨` (not in candidates.tsv)
- `着笔` (not in candidates.tsv)
- `穿着打扮` (not in candidates.tsv)
- `附着力` (not in candidates.tsv)
- `胶着状态` (not in candidates.tsv)
- `着重号` (not in candidates.tsv)
- `着眼于` (not in candidates.tsv)

### Full word list (all 878 rows, sorted by freq desc)

| freq | word | typed_code | correct_code | dir |
|---|---|---|---|---|
| 34440 | `看着` | `kanzhao` | `kanzhe` | `zhe`→`zhao` |
| 34440 | `看着` | `kanzhuo` | `kanzhe` | `zhe`→`zhuo` |
| 34078 | `随着` | `suizhao` | `suizhe` | `zhe`→`zhao` |
| 34078 | `随着` | `suizhuo` | `suizhe` | `zhe`→`zhuo` |
| 32851 | `不着` | `buzhao` | `buzhe` | `zhe`→`zhao` |
| 32851 | `不着` | `buzhuo` | `buzhe` | `zhe`→`zhuo` |
| 30390 | `睡不着` | `shuibuzhao` | `shuibuzhe` | `zhe`→`zhao` |
| 30390 | `睡不着` | `shuibuzhuo` | `shuibuzhe` | `zhe`→`zhuo` |
| 29804 | `背着` | `beizhao` | `beizhe` | `zhe`→`zhao` |
| 29804 | `背着` | `beizhuo` | `beizhe` | `zhe`→`zhuo` |
| 27491 | `趁着` | `chenzhao` | `chenzhe` | `zhe`→`zhao` |
| 27491 | `趁着` | `chenzhuo` | `chenzhe` | `zhe`→`zhuo` |
| 27376 | `对着` | `duizhao` | `duizhe` | `zhe`→`zhao` |
| 27376 | `对着` | `duizhuo` | `duizhe` | `zhe`→`zhuo` |
| 26724 | `顺着` | `shunzhao` | `shunzhe` | `zhe`→`zhao` |
| 26724 | `顺着` | `shunzhuo` | `shunzhe` | `zhe`→`zhuo` |
| 26579 | `连着` | `lianzhao` | `lianzhe` | `zhe`→`zhao` |
| 26579 | `连着` | `lianzhuo` | `lianzhe` | `zhe`→`zhuo` |
| 26028 | `用不着` | `yongbuzhao` | `yongbuzhe` | `zhe`→`zhao` |
| 26028 | `用不着` | `yongbuzhuo` | `yongbuzhe` | `zhe`→`zhuo` |
| 25809 | `觉着` | `juezhao` | `juezhe` | `zhe`→`zhao` |
| 25809 | `觉着` | `juezhuo` | `juezhe` | `zhe`→`zhuo` |
| 25786 | `着实` | `zhaoshi` | `zheshi` | `zhe`→`zhao` |
| 25786 | `着实` | `zhuoshi` | `zheshi` | `zhe`→`zhuo` |
| 25711 | `怀着` | `huaizhao` | `huaizhe` | `zhe`→`zhao` |
| 25711 | `怀着` | `huaizhuo` | `huaizhe` | `zhe`→`zhuo` |
| 25152 | `当着` | `dangzhao` | `dangzhe` | `zhe`→`zhao` |
| 25152 | `当着` | `dangzhuo` | `dangzhe` | `zhe`→`zhuo` |
| 24631 | `挨着` | `aizhao` | `aizhe` | `zhe`→`zhao` |
| 24631 | `挨着` | `aizhuo` | `aizhe` | `zhe`→`zhuo` |
| 23674 | `打着` | `dazhao` | `dazhe` | `zhe`→`zhao` |
| 23674 | `打着` | `dazhuo` | `dazhe` | `zhe`→`zhuo` |
| 23565 | `闲着` | `xianzhao` | `xianzhe` | `zhe`→`zhao` |
| 23565 | `闲着` | `xianzhuo` | `xianzhe` | `zhe`→`zhuo` |
| 23007 | `凭着` | `pingzhao` | `pingzhe` | `zhe`→`zhao` |
| 23007 | `凭着` | `pingzhuo` | `pingzhe` | `zhe`→`zhuo` |
| 22756 | `着上` | `zhaoshang` | `zheshang` | `zhe`→`zhao` |
| 22756 | `着上` | `zhuoshang` | `zheshang` | `zhe`→`zhuo` |
| 22463 | `别着` | `biezhao` | `biezhe` | `zhe`→`zhao` |
| 22463 | `别着` | `biezhuo` | `biezhe` | `zhe`→`zhuo` |
| 22328 | `指着` | `zhizhao` | `zhizhe` | `zhe`→`zhao` |
| 22328 | `指着` | `zhizhuo` | `zhizhe` | `zhe`→`zhuo` |
| 22249 | `冲着` | `chongzhao` | `chongzhe` | `zhe`→`zhao` |
| 22249 | `冲着` | `chongzhuo` | `chongzhe` | `zhe`→`zhuo` |
| 22110 | `围着` | `weizhao` | `weizhe` | `zhe`→`zhao` |
| 22110 | `围着` | `weizhuo` | `weizhe` | `zhe`→`zhuo` |
| 21874 | `憋着` | `biezhao` | `biezhe` | `zhe`→`zhao` |
| 21874 | `憋着` | `biezhuo` | `biezhe` | `zhe`→`zhuo` |
| 21807 | `着地` | `zhaode` | `zhedi` | `zhe`→`zhao` |
| 21807 | `着地` | `zhaodi` | `zhedi` | `zhe`→`zhao` |
| 21807 | `着地` | `zhuode` | `zhedi` | `zhe`→`zhuo` |
| 21807 | `着地` | `zhuodi` | `zhedi` | `zhe`→`zhuo` |
| 21720 | `捂着` | `wuzhao` | `wuzhe` | `zhe`→`zhao` |
| 21720 | `捂着` | `wuzhuo` | `wuzhe` | `zhe`→`zhuo` |
| 21560 | `发着` | `fazhao` | `fazhe` | `zhe`→`zhao` |
| 21560 | `发着` | `fazhuo` | `fazhe` | `zhe`→`zhuo` |
| 21431 | `看着办` | `kanzhaoban` | `kanzheban` | `zhe`→`zhao` |
| 21431 | `看着办` | `kanzhuoban` | `kanzheban` | `zhe`→`zhuo` |
| 21378 | `提着` | `tizhao` | `tizhe` | `zhe`→`zhao` |
| 21378 | `提着` | `tizhuo` | `tizhe` | `zhe`→`zhuo` |
| 21351 | `原着` | `yuanzhao` | `yuanzhe` | `zhe`→`zhao` |
| 21351 | `原着` | `yuanzhuo` | `yuanzhe` | `zhe`→`zhuo` |
| 21312 | `身着` | `shenzhao` | `shenzhe` | `zhe`→`zhao` |
| 21312 | `身着` | `shenzhuo` | `shenzhe` | `zhe`→`zhuo` |
| 21100 | `下着` | `xiazhao` | `xiazhe` | `zhe`→`zhao` |
| 21100 | `下着` | `xiazhuo` | `xiazhe` | `zhe`→`zhuo` |
| 21047 | `存着` | `cunzhao` | `cunzhe` | `zhe`→`zhao` |
| 21047 | `存着` | `cunzhuo` | `cunzhe` | `zhe`→`zhuo` |
| 20780 | `趴着` | `pazhao` | `pazhe` | `zhe`→`zhao` |
| 20780 | `趴着` | `pazhuo` | `pazhe` | `zhe`→`zhuo` |
| 20703 | `光着` | `guangzhao` | `guangzhe` | `zhe`→`zhao` |
| 20703 | `光着` | `guangzhuo` | `guangzhe` | `zhe`→`zhuo` |
| 20586 | `披着` | `pizhao` | `pizhe` | `zhe`→`zhao` |
| 20586 | `披着` | `pizhuo` | `pizhe` | `zhe`→`zhuo` |
| 20444 | `悠着` | `youzhao` | `youzhe` | `zhe`→`zhao` |
| 20444 | `悠着` | `youzhuo` | `youzhe` | `zhe`→`zhuo` |
| 20431 | `惦记着` | `dianjizhao` | `dianjizhe` | `zhe`→`zhao` |
| 20431 | `惦记着` | `dianjizhuo` | `dianjizhe` | `zhe`→`zhuo` |
| 20341 | `找不着` | `zhaobuzhao` | `zhaobuzhe` | `zhe`→`zhao` |
| 20341 | `找不着` | `zhaobuzhuo` | `zhaobuzhe` | `zhe`→`zhuo` |
| 20272 | `悠着点` | `youzhaodian` | `youzhedian` | `zhe`→`zhao` |
| 20272 | `悠着点` | `youzhuodian` | `youzhedian` | `zhe`→`zhuo` |
| 20190 | `多着` | `duozhao` | `duozhe` | `zhe`→`zhao` |
| 20190 | `多着` | `duozhuo` | `duozhe` | `zhe`→`zhuo` |
| 20130 | `烧着` | `shaozhao` | `shaozhe` | `zhe`→`zhao` |
| 20130 | `烧着` | `shaozhuo` | `shaozhe` | `zhe`→`zhuo` |
| 19846 | `乘着` | `chengzhao` | `chengzhe` | `zhe`→`zhao` |
| 19846 | `乘着` | `chengzhuo` | `chengzhe` | `zhe`→`zhuo` |
| 19846 | `上着` | `shangzhao` | `shangzhe` | `zhe`→`zhao` |
| 19846 | `上着` | `shangzhuo` | `shangzhe` | `zhe`→`zhuo` |
| 19792 | `借着` | `jiezhao` | `jiezhe` | `zhe`→`zhao` |
| 19792 | `借着` | `jiezhuo` | `jiezhe` | `zhe`→`zhuo` |
| 19500 | `扛着` | `kangzhao` | `kangzhe` | `zhe`→`zhao` |
| 19500 | `扛着` | `kangzhuo` | `kangzhe` | `zhe`→`zhuo` |
| 19371 | `划着` | `huazhao` | `huazhe` | `zhe`→`zhao` |
| 19371 | `划着` | `huazhuo` | `huazhe` | `zhe`→`zhuo` |
| 19052 | `聊着` | `liaozhao` | `liaozhe` | `zhe`→`zhao` |
| 19052 | `聊着` | `liaozhuo` | `liaozhe` | `zhe`→`zhuo` |
| 18919 | `包着` | `baozhao` | `baozhe` | `zhe`→`zhao` |
| 18919 | `包着` | `baozhuo` | `baozhe` | `zhe`→`zhuo` |
| 18891 | `显着` | `xianzhao` | `xianzhe` | `zhe`→`zhao` |
| 18891 | `显着` | `xianzhuo` | `xianzhe` | `zhe`→`zhuo` |
| 18886 | `排着` | `paizhao` | `paizhe` | `zhe`→`zhao` |
| 18886 | `排着` | `paizhuo` | `paizhe` | `zhe`→`zhuo` |
| 18868 | `按着` | `anzhao` | `anzhe` | `zhe`→`zhao` |
| 18868 | `按着` | `anzhuo` | `anzhe` | `zhe`→`zhuo` |
| 18714 | `用得着` | `yongdezhao` | `yongdezhe` | `zhe`→`zhao` |
| 18714 | `用得着` | `yongdezhuo` | `yongdezhe` | `zhe`→`zhuo` |
| 18697 | `攒着` | `zanzhao` | `zanzhe` | `zhe`→`zhao` |
| 18697 | `攒着` | `zanzhuo` | `zanzhe` | `zhe`→`zhuo` |
| 18260 | `推着` | `tuizhao` | `tuizhe` | `zhe`→`zhao` |
| 18260 | `推着` | `tuizhuo` | `tuizhe` | `zhe`→`zhuo` |
| 18242 | `数着` | `shuzhao` | `shuzhe` | `zhe`→`zhao` |
| 18242 | `数着` | `shuzhuo` | `shuzhe` | `zhe`→`zhuo` |
| 18237 | `闲着没事` | `xianzhaomeishi` | `xianzhemeishi` | `zhe`→`zhao` |
| 18237 | `闲着没事` | `xianzhuomeishi` | `xianzhemeishi` | `zhe`→`zhuo` |
| 17961 | `着作` | `zhaozuo` | `zhezuo` | `zhe`→`zhao` |
| 17961 | `着作` | `zhuozuo` | `zhezuo` | `zhe`→`zhuo` |
| 17947 | `耗着` | `haozhao` | `haozhe` | `zhe`→`zhao` |
| 17947 | `耗着` | `haozhuo` | `haozhe` | `zhe`→`zhuo` |
| 17808 | `多着呢` | `duozhaone` | `duozhene` | `zhe`→`zhao` |
| 17808 | `多着呢` | `duozhaoni` | `duozhene` | `zhe`→`zhao` |
| 17808 | `多着呢` | `duozhuone` | `duozhene` | `zhe`→`zhuo` |
| 17808 | `多着呢` | `duozhuoni` | `duozhene` | `zhe`→`zhuo` |
| 17600 | `粘着` | `zhanzhao` | `zhanzhe` | `zhe`→`zhao` |
| 17600 | `粘着` | `zhanzhuo` | `zhanzhe` | `zhe`→`zhuo` |
| 17519 | `陪伴着` | `peibanzhao` | `peibanzhe` | `zhe`→`zhao` |
| 17519 | `陪伴着` | `peibanzhuo` | `peibanzhe` | `zhe`→`zhuo` |
| 17435 | `偷着乐` | `touzhaole` | `touzhele` | `zhe`→`zhao` |
| 17435 | `偷着乐` | `touzhaoyue` | `touzhele` | `zhe`→`zhao` |
| 17435 | `偷着乐` | `touzhuole` | `touzhele` | `zhe`→`zhuo` |
| 17435 | `偷着乐` | `touzhuoyue` | `touzhele` | `zhe`→`zhuo` |
| 17426 | `下着雨` | `xiazhaoyu` | `xiazheyu` | `zhe`→`zhao` |
| 17426 | `下着雨` | `xiazhuoyu` | `xiazheyu` | `zhe`→`zhuo` |
| 17386 | `挽着` | `wanzhao` | `wanzhe` | `zhe`→`zhao` |
| 17386 | `挽着` | `wanzhuo` | `wanzhe` | `zhe`→`zhuo` |
| 17371 | `管不着` | `guanbuzhao` | `guanbuzhe` | `zhe`→`zhao` |
| 17371 | `管不着` | `guanbuzhuo` | `guanbuzhe` | `zhe`→`zhuo` |
| 17172 | `学着点` | `xuezhaodian` | `xuezhedian` | `zhe`→`zhao` |
| 17172 | `学着点` | `xuezhuodian` | `xuezhedian` | `zhe`→`zhuo` |
| 17164 | `打不着` | `dabuzhao` | `dabuzhe` | `zhe`→`zhao` |
| 17164 | `打不着` | `dabuzhuo` | `dabuzhe` | `zhe`→`zhuo` |
| 17150 | `够不着` | `goubuzhao` | `goubuzhe` | `zhe`→`zhao` |
| 17150 | `够不着` | `goubuzhuo` | `goubuzhe` | `zhe`→`zhuo` |
| 17029 | `见不着` | `jianbuzhao` | `jianbuzhe` | `zhe`→`zhao` |
| 17029 | `见不着` | `jianbuzhuo` | `jianbuzhe` | `zhe`→`zhuo` |
| 16900 | `盼望着` | `panwangzhao` | `panwangzhe` | `zhe`→`zhao` |
| 16900 | `盼望着` | `panwangzhuo` | `panwangzhe` | `zhe`→`zhuo` |
| 16840 | `昧着良心` | `meizhaoliangxin` | `meizheliangxin` | `zhe`→`zhao` |
| 16840 | `昧着良心` | `meizhuoliangxin` | `meizheliangxin` | `zhe`→`zhuo` |
| 16824 | `饿着肚子` | `ezhaoduzi` | `ezheduzi` | `zhe`→`zhao` |
| 16824 | `饿着肚子` | `ezhuoduzi` | `ezheduzi` | `zhe`→`zhuo` |
| 16789 | `含着泪` | `hanzhaolei` | `hanzhelei` | `zhe`→`zhao` |
| 16789 | `含着泪` | `hanzhuolei` | `hanzhelei` | `zhe`→`zhuo` |
| 16737 | `着名` | `zhaoming` | `zheming` | `zhe`→`zhao` |
| 16737 | `着名` | `zhuoming` | `zheming` | `zhe`→`zhuo` |
| 16729 | `趴着睡` | `pazhaoshui` | `pazheshui` | `zhe`→`zhao` |
| 16729 | `趴着睡` | `pazhuoshui` | `pazheshui` | `zhe`→`zhuo` |
| 16669 | `正着` | `zhengzhao` | `zhengzhe` | `zhe`→`zhao` |
| 16669 | `正着` | `zhengzhuo` | `zhengzhe` | `zhe`→`zhuo` |
| 16601 | `揣着` | `chuaizhao` | `chuaizhe` | `zhe`→`zhao` |
| 16601 | `揣着` | `chuaizhuo` | `chuaizhe` | `zhe`→`zhuo` |
| 16590 | `引着` | `yinzhao` | `yinzhe` | `zhe`→`zhao` |
| 16590 | `引着` | `yinzhuo` | `yinzhe` | `zhe`→`zhuo` |
| 16580 | `关着` | `guanzhao` | `guanzhe` | `zhe`→`zhao` |
| 16580 | `关着` | `guanzhuo` | `guanzhe` | `zhe`→`zhuo` |
| 16532 | `深爱着` | `shenaizhao` | `shenaizhe` | `zhe`→`zhao` |
| 16532 | `深爱着` | `shenaizhuo` | `shenaizhe` | `zhe`→`zhuo` |
| 16431 | `转着` | `zhuanzhao` | `zhuanzhe` | `zhe`→`zhao` |
| 16431 | `转着` | `zhuanzhuo` | `zhuanzhe` | `zhe`→`zhuo` |
| 16359 | `担着` | `danzhao` | `danzhe` | `zhe`→`zhao` |
| 16359 | `担着` | `danzhuo` | `danzhe` | `zhe`→`zhuo` |
| 16250 | `开着车` | `kaizhaoche` | `kaizheche` | `zhe`→`zhao` |
| 16250 | `开着车` | `kaizhuoche` | `kaizheche` | `zhe`→`zhuo` |
| 16121 | `比着` | `bizhao` | `bizhe` | `zhe`→`zhao` |
| 16121 | `比着` | `bizhuo` | `bizhe` | `zhe`→`zhuo` |
| 15986 | `一着` | `yizhao` | `yizhe` | `zhe`→`zhao` |
| 15986 | `一着` | `yizhuo` | `yizhe` | `zhe`→`zhuo` |
| 15969 | `舔着` | `tianzhao` | `tianzhe` | `zhe`→`zhao` |
| 15969 | `舔着` | `tianzhuo` | `tianzhe` | `zhe`→`zhuo` |
| 15947 | `弥漫着` | `mimanzhao` | `mimanzhe` | `zhe`→`zhao` |
| 15947 | `弥漫着` | `mimanzhuo` | `mimanzhe` | `zhe`→`zhuo` |
| 15900 | `垫着` | `dianzhao` | `dianzhe` | `zhe`→`zhao` |
| 15900 | `垫着` | `dianzhuo` | `dianzhe` | `zhe`→`zhuo` |
| 15869 | `洋溢着` | `yangyizhao` | `yangyizhe` | `zhe`→`zhao` |
| 15869 | `洋溢着` | `yangyizhuo` | `yangyizhe` | `zhe`→`zhuo` |
| 15795 | `厚着脸皮` | `houzhaolianpi` | `houzhelianpi` | `zhe`→`zhao` |
| 15795 | `厚着脸皮` | `houzhuolianpi` | `houzhelianpi` | `zhe`→`zhuo` |
| 15747 | `噎着` | `yezhao` | `yezhe` | `zhe`→`zhao` |
| 15747 | `噎着` | `yezhuo` | `yezhe` | `zhe`→`zhuo` |
| 15742 | `翘着` | `qiaozhao` | `qiaozhe` | `zhe`→`zhao` |
| 15742 | `翘着` | `qiaozhuo` | `qiaozhe` | `zhe`→`zhuo` |
| 15698 | `低着头` | `dizhaotou` | `dizhetou` | `zhe`→`zhao` |
| 15698 | `低着头` | `dizhuotou` | `dizhetou` | `zhe`→`zhuo` |
| 15688 | `演着` | `yanzhao` | `yanzhe` | `zhe`→`zhao` |
| 15688 | `演着` | `yanzhuo` | `yanzhe` | `zhe`→`zhuo` |
| 15615 | `着着` | `zhaozhao` | `zhezhe` | `zhe`→`zhao` |
| 15615 | `着着` | `zhaozhe` | `zhezhe` | `zhe`→`zhao` |
| 15615 | `着着` | `zhaozhuo` | `zhezhe` | `zhe`→`zhao` |
| 15615 | `着着` | `zhezhao` | `zhezhe` | `zhe`→`zhao` |
| 15615 | `着着` | `zhezhuo` | `zhezhe` | `zhe`→`zhuo` |
| 15615 | `着着` | `zhuozhao` | `zhezhe` | `zhe`→`zhuo` |
| 15615 | `着着` | `zhuozhe` | `zhezhe` | `zhe`→`zhuo` |
| 15615 | `着着` | `zhuozhuo` | `zhezhe` | `zhe`→`zhuo` |
| 15524 | `夹杂着` | `jiazazhao` | `jiazazhe` | `zhe`→`zhao` |
| 15524 | `夹杂着` | `jiazazhuo` | `jiazazhe` | `zhe`→`zhuo` |
| 15516 | `套着` | `taozhao` | `taozhe` | `zhe`→`zhao` |
| 15516 | `套着` | `taozhuo` | `taozhe` | `zhe`→`zhuo` |
| 15500 | `手拿着` | `shounazhao` | `shounazhe` | `zhe`→`zhao` |
| 15500 | `手拿着` | `shounazhuo` | `shounazhe` | `zhe`→`zhuo` |
| 15311 | `防着` | `fangzhao` | `fangzhe` | `zhe`→`zhao` |
| 15311 | `防着` | `fangzhuo` | `fangzhe` | `zhe`→`zhuo` |
| 15190 | `掐着` | `qiazhao` | `qiazhe` | `zhe`→`zhao` |
| 15190 | `掐着` | `qiazhuo` | `qiazhe` | `zhe`→`zhuo` |
| 15168 | `拴着` | `shuanzhao` | `shuanzhe` | `zhe`→`zhao` |
| 15168 | `拴着` | `shuanzhuo` | `shuanzhe` | `zhe`→`zhuo` |
| 15046 | `着床` | `zhaochuang` | `zhechuang` | `zhe`→`zhao` |
| 15046 | `着床` | `zhuochuang` | `zhechuang` | `zhe`→`zhuo` |
| 15033 | `啃着` | `kenzhao` | `kenzhe` | `zhe`→`zhao` |
| 15033 | `啃着` | `kenzhuo` | `kenzhe` | `zhe`→`zhuo` |
| 15013 | `唱着歌` | `changzhaoge` | `changzhege` | `zhe`→`zhao` |
| 15013 | `唱着歌` | `changzhuoge` | `changzhege` | `zhe`→`zhuo` |
| 14847 | `考着` | `kaozhao` | `kaozhe` | `zhe`→`zhao` |
| 14847 | `考着` | `kaozhuo` | `kaozhe` | `zhe`→`zhuo` |
| 14847 | `咬着牙` | `yaozhaoya` | `yaozheya` | `zhe`→`zhao` |
| 14847 | `咬着牙` | `yaozhuoya` | `yaozheya` | `zhe`→`zhuo` |
| 14751 | `枕着` | `zhenzhao` | `zhenzhe` | `zhe`→`zhao` |
| 14751 | `枕着` | `zhenzhuo` | `zhenzhe` | `zhe`→`zhuo` |
| 14694 | `等着瞧` | `dengzhaoqiao` | `dengzheqiao` | `zhe`→`zhao` |
| 14694 | `等着瞧` | `dengzhuoqiao` | `dengzheqiao` | `zhe`→`zhuo` |
| 14586 | `病着` | `bingzhao` | `bingzhe` | `zhe`→`zhao` |
| 14586 | `病着` | `bingzhuo` | `bingzhe` | `zhe`→`zhuo` |
| 14505 | `红着脸` | `hongzhaolian` | `hongzhelian` | `zhe`→`zhao` |
| 14505 | `红着脸` | `hongzhuolian` | `hongzhelian` | `zhe`→`zhuo` |
| 14486 | `流着泪` | `liuzhaolei` | `liuzhelei` | `zhe`→`zhao` |
| 14486 | `流着泪` | `liuzhuolei` | `liuzhelei` | `zhe`→`zhuo` |
| 14450 | `背对着` | `beiduizhao` | `beiduizhe` | `zhe`→`zhao` |
| 14450 | `背对着` | `beiduizhuo` | `beiduizhe` | `zhe`→`zhuo` |
| 14423 | `着紧` | `zhaojin` | `zhejin` | `zhe`→`zhao` |
| 14423 | `着紧` | `zhuojin` | `zhejin` | `zhe`→`zhuo` |
| 14319 | `正对着` | `zhengduizhao` | `zhengduizhe` | `zhe`→`zhao` |
| 14319 | `正对着` | `zhengduizhuo` | `zhengduizhe` | `zhe`→`zhuo` |
| 14257 | `手牵着` | `shouqianzhao` | `shouqianzhe` | `zhe`→`zhao` |
| 14257 | `手牵着` | `shouqianzhuo` | `shouqianzhe` | `zhe`→`zhuo` |
| 14210 | `肩负着` | `jianfuzhao` | `jianfuzhe` | `zhe`→`zhao` |
| 14210 | `肩负着` | `jianfuzhuo` | `jianfuzhe` | `zhe`→`zhuo` |
| 14168 | `忍受着` | `renshouzhao` | `renshouzhe` | `zhe`→`zhao` |
| 14168 | `忍受着` | `renshouzhuo` | `renshouzhe` | `zhe`→`zhuo` |
| 14072 | `背负着` | `beifuzhao` | `beifuzhe` | `zhe`→`zhao` |
| 14072 | `背负着` | `beifuzhuo` | `beifuzhe` | `zhe`→`zhuo` |
| 14054 | `拄着` | `zhuzhao` | `zhuzhe` | `zhe`→`zhao` |
| 14054 | `拄着` | `zhuzhuo` | `zhuzhe` | `zhe`→`zhuo` |
| 13989 | `敷着` | `fuzhao` | `fuzhe` | `zhe`→`zhao` |
| 13989 | `敷着` | `fuzhuo` | `fuzhe` | `zhe`→`zhuo` |
| 13763 | `还着` | `haizhao` | `haizhe` | `zhe`→`zhao` |
| 13763 | `还着` | `haizhuo` | `haizhe` | `zhe`→`zhuo` |
| 13728 | `夹着尾巴` | `jiazhaoweiba` | `jiazheweiba` | `zhe`→`zhao` |
| 13728 | `夹着尾巴` | `jiazhuoweiba` | `jiazheweiba` | `zhe`→`zhuo` |
| 13728 | `漂着` | `piaozhao` | `piaozhe` | `zhe`→`zhao` |
| 13728 | `漂着` | `piaozhuo` | `piaozhe` | `zhe`→`zhuo` |
| 13690 | `牵着鼻子` | `qianzhaobizi` | `qianzhebizi` | `zhe`→`zhao` |
| 13690 | `牵着鼻子` | `qianzhuobizi` | `qianzhebizi` | `zhe`→`zhuo` |
| 13650 | `搁着` | `gezhao` | `gezhe` | `zhe`→`zhao` |
| 13650 | `搁着` | `gezhuo` | `gezhe` | `zhe`→`zhuo` |
| 13630 | `下着雪` | `xiazhaoxue` | `xiazhexue` | `zhe`→`zhao` |
| 13630 | `下着雪` | `xiazhuoxue` | `xiazhexue` | `zhe`→`zhuo` |
| 13630 | `追随着` | `zhuisuizhao` | `zhuisuizhe` | `zhe`→`zhao` |
| 13630 | `追随着` | `zhuisuizhuo` | `zhuisuizhe` | `zhe`→`zhuo` |
| 13410 | `摸得着` | `modezhao` | `modezhe` | `zhe`→`zhao` |
| 13410 | `摸得着` | `modezhuo` | `modezhe` | `zhe`→`zhuo` |
| 13346 | `蕴含着` | `yunhanzhao` | `yunhanzhe` | `zhe`→`zhao` |
| 13346 | `蕴含着` | `yunhanzhuo` | `yunhanzhe` | `zhe`→`zhuo` |
| 13311 | `正忙着` | `zhengmangzhao` | `zhengmangzhe` | `zhe`→`zhao` |
| 13311 | `正忙着` | `zhengmangzhuo` | `zhengmangzhe` | `zhe`→`zhuo` |
| 13246 | `拥着` | `yongzhao` | `yongzhe` | `zhe`→`zhao` |
| 13246 | `拥着` | `yongzhuo` | `yongzhe` | `zhe`→`zhuo` |
| 13184 | `着录` | `zhaolu` | `zhelu` | `zhe`→`zhao` |
| 13184 | `着录` | `zhuolu` | `zhelu` | `zhe`→`zhuo` |
| 13159 | `架着` | `jiazhao` | `jiazhe` | `zhe`→`zhao` |
| 13159 | `架着` | `jiazhuo` | `jiazhe` | `zhe`→`zhuo` |
| 13119 | `找不着北` | `zhaobuzhaobei` | `zhaobuzhebei` | `zhe`→`zhao` |
| 13119 | `找不着北` | `zhaobuzhuobei` | `zhaobuzhebei` | `zhe`→`zhuo` |
| 13010 | `绷着` | `bengzhao` | `bengzhe` | `zhe`→`zhao` |
| 13010 | `绷着` | `bengzhuo` | `bengzhe` | `zhe`→`zhuo` |
| 12949 | `直着` | `zhizhao` | `zhizhe` | `zhe`→`zhao` |
| 12949 | `直着` | `zhizhuo` | `zhizhe` | `zhe`→`zhuo` |
| 12939 | `捂着脸` | `wuzhaolian` | `wuzhelian` | `zhe`→`zhao` |
| 12939 | `捂着脸` | `wuzhuolian` | `wuzhelian` | `zhe`→`zhuo` |
| 12892 | `挎着` | `kuazhao` | `kuazhe` | `zhe`→`zhao` |
| 12892 | `挎着` | `kuazhuo` | `kuazhe` | `zhe`→`zhuo` |
| 12879 | `板着脸` | `banzhaolian` | `banzhelian` | `zhe`→`zhao` |
| 12879 | `板着脸` | `banzhuolian` | `banzhelian` | `zhe`→`zhuo` |
| 12871 | `拥抱着` | `yongbaozhao` | `yongbaozhe` | `zhe`→`zhao` |
| 12871 | `拥抱着` | `yongbaozhuo` | `yongbaozhe` | `zhe`→`zhuo` |
| 12828 | `呛着` | `qiangzhao` | `qiangzhe` | `zhe`→`zhao` |
| 12828 | `呛着` | `qiangzhuo` | `qiangzhe` | `zhe`→`zhuo` |
| 12673 | `踮着` | `dianzhao` | `dianzhe` | `zhe`→`zhao` |
| 12673 | `踮着` | `dianzhuo` | `dianzhe` | `zhe`→`zhuo` |
| 12637 | `闪耀着` | `shanyaozhao` | `shanyaozhe` | `zhe`→`zhao` |
| 12637 | `闪耀着` | `shanyaozhuo` | `shanyaozhe` | `zhe`→`zhuo` |
| 12632 | `猜不着` | `caibuzhao` | `caibuzhe` | `zhe`→`zhao` |
| 12632 | `猜不着` | `caibuzhuo` | `caibuzhe` | `zhe`→`zhuo` |
| 12632 | `捞着` | `laozhao` | `laozhe` | `zhe`→`zhao` |
| 12632 | `捞着` | `laozhuo` | `laozhe` | `zhe`→`zhuo` |
| 12608 | `闪烁着` | `shanshuozhao` | `shanshuozhe` | `zhe`→`zhao` |
| 12608 | `闪烁着` | `shanshuozhuo` | `shanshuozhe` | `zhe`→`zhuo` |
| 12591 | `晾着` | `liangzhao` | `liangzhe` | `zhe`→`zhao` |
| 12591 | `晾着` | `liangzhuo` | `liangzhe` | `zhe`→`zhuo` |
| 12575 | `着称` | `zhaochen` | `zhecheng` | `zhe`→`zhao` |
| 12575 | `着称` | `zhaocheng` | `zhecheng` | `zhe`→`zhao` |
| 12575 | `着称` | `zhuochen` | `zhecheng` | `zhe`→`zhuo` |
| 12575 | `着称` | `zhuocheng` | `zhecheng` | `zhe`→`zhuo` |
| 12531 | `诉说着` | `sushuozhao` | `sushuozhe` | `zhe`→`zhao` |
| 12531 | `诉说着` | `sushuozhuo` | `sushuozhe` | `zhe`→`zhuo` |
| 12530 | `盘算着` | `pansuanzhao` | `pansuanzhe` | `zhe`→`zhao` |
| 12530 | `盘算着` | `pansuanzhuo` | `pansuanzhe` | `zhe`→`zhuo` |
| 12464 | `抓着不放` | `zhuazhaobufang` | `zhuazhebufang` | `zhe`→`zhao` |
| 12464 | `抓着不放` | `zhuazhuobufang` | `zhuazhebufang` | `zhe`→`zhuo` |
| 12447 | `映着` | `yingzhao` | `yingzhe` | `zhe`→`zhao` |
| 12447 | `映着` | `yingzhuo` | `yingzhe` | `zhe`→`zhuo` |
| 12446 | `兜着走` | `douzhaozou` | `douzhezou` | `zhe`→`zhao` |
| 12446 | `兜着走` | `douzhuozou` | `douzhezou` | `zhe`→`zhuo` |
| 12446 | `站着不动` | `zhanzhaobudong` | `zhanzhebudong` | `zhe`→`zhao` |
| 12446 | `站着不动` | `zhanzhuobudong` | `zhanzhebudong` | `zhe`→`zhuo` |
| 12424 | `炖着` | `dunzhao` | `dunzhe` | `zhe`→`zhao` |
| 12424 | `炖着` | `dunzhuo` | `dunzhe` | `zhe`→`zhuo` |
| 12413 | `环绕着` | `huanraozhao` | `huanraozhe` | `zhe`→`zhao` |
| 12413 | `环绕着` | `huanraozhuo` | `huanraozhe` | `zhe`→`zhuo` |
| 12408 | `靠着` | `kaozhao` | `kaozhe` | `zhe`→`zhao` |
| 12408 | `靠着` | `kaozhuo` | `kaozhe` | `zhe`→`zhuo` |
| 12330 | `平躺着` | `pingtangzhao` | `pingtangzhe` | `zhe`→`zhao` |
| 12330 | `平躺着` | `pingtangzhuo` | `pingtangzhe` | `zhe`→`zhuo` |
| 12325 | `扮演着` | `banyanzhao` | `banyanzhe` | `zhe`→`zhao` |
| 12325 | `扮演着` | `banyanzhuo` | `banyanzhe` | `zhe`→`zhuo` |
| 12275 | `歪着头` | `waizhaotou` | `waizhetou` | `zhe`→`zhao` |
| 12275 | `歪着头` | `waizhuotou` | `waizhetou` | `zhe`→`zhuo` |
| 12234 | `笼罩着` | `longzhaozhao` | `longzhaozhe` | `zhe`→`zhao` |
| 12234 | `笼罩着` | `longzhaozhuo` | `longzhaozhe` | `zhe`→`zhuo` |
| 12206 | `暗示着` | `anshizhao` | `anshizhe` | `zhe`→`zhao` |
| 12206 | `暗示着` | `anshizhuo` | `anshizhe` | `zhe`→`zhuo` |
| 12189 | `够得着` | `goudezhao` | `goudezhe` | `zhe`→`zhao` |
| 12189 | `够得着` | `goudezhuo` | `goudezhe` | `zhe`→`zhuo` |
| 12150 | `迈着` | `maizhao` | `maizhe` | `zhe`→`zhao` |
| 12150 | `迈着` | `maizhuo` | `maizhe` | `zhe`→`zhuo` |
| 12092 | `弯着腰` | `wanzhaoyao` | `wanzheyao` | `zhe`→`zhao` |
| 12092 | `弯着腰` | `wanzhuoyao` | `wanzheyao` | `zhe`→`zhuo` |
| 12078 | `凭借着` | `pingjiezhao` | `pingjiezhe` | `zhe`→`zhao` |
| 12078 | `凭借着` | `pingjiezhuo` | `pingjiezhe` | `zhe`→`zhuo` |
| 12071 | `硬着头皮` | `yingzhaotoupi` | `yingzhetoupi` | `zhe`→`zhao` |
| 12071 | `硬着头皮` | `yingzhuotoupi` | `yingzhetoupi` | `zhe`→`zhuo` |
| 11987 | `搭配着` | `dapeizhao` | `dapeizhe` | `zhe`→`zhao` |
| 11987 | `搭配着` | `dapeizhuo` | `dapeizhe` | `zhe`→`zhuo` |
| 11888 | `糊着` | `huzhao` | `huzhe` | `zhe`→`zhao` |
| 11888 | `糊着` | `huzhuo` | `huzhe` | `zhe`→`zhuo` |
| 11867 | `手抱着` | `shoubaozhao` | `shoubaozhe` | `zhe`→`zhao` |
| 11867 | `手抱着` | `shoubaozhuo` | `shoubaozhe` | `zhe`→`zhuo` |
| 11768 | `捏着鼻子` | `niezhaobizi` | `niezhebizi` | `zhe`→`zhao` |
| 11768 | `捏着鼻子` | `niezhuobizi` | `niezhebizi` | `zhe`→`zhuo` |
| 11651 | `压抑着` | `yayizhao` | `yayizhe` | `zhe`→`zhao` |
| 11651 | `压抑着` | `yayizhuo` | `yayizhe` | `zhe`→`zhuo` |
| 11579 | `打着灯笼` | `dazhaodenglong` | `dazhedenglong` | `zhe`→`zhao` |
| 11579 | `打着灯笼` | `dazhuodenglong` | `dazhedenglong` | `zhe`→`zhuo` |
| 11550 | `敞着` | `changzhao` | `changzhe` | `zhe`→`zhao` |
| 11550 | `敞着` | `changzhuo` | `changzhe` | `zhe`→`zhuo` |
| 11528 | `整着` | `zhengzhao` | `zhengzhe` | `zhe`→`zhao` |
| 11528 | `整着` | `zhengzhuo` | `zhengzhe` | `zhe`→`zhuo` |
| 11514 | `秉着` | `bingzhao` | `bingzhe` | `zhe`→`zhao` |
| 11514 | `秉着` | `bingzhuo` | `bingzhe` | `zhe`→`zhuo` |
| 11502 | `咧着` | `liezhao` | `liezhe` | `zhe`→`zhao` |
| 11502 | `咧着` | `liezhuo` | `liezhe` | `zhe`→`zhuo` |
| 11424 | `因着` | `yinzhao` | `yinzhe` | `zhe`→`zhao` |
| 11424 | `因着` | `yinzhuo` | `yinzhe` | `zhe`→`zhuo` |
| 11332 | `霸着` | `bazhao` | `bazhe` | `zhe`→`zhao` |
| 11332 | `霸着` | `bazhuo` | `bazhe` | `zhe`→`zhuo` |
| 11332 | `摊着` | `tanzhao` | `tanzhe` | `zhe`→`zhao` |
| 11332 | `摊着` | `tanzhuo` | `tanzhe` | `zhe`→`zhuo` |
| 11268 | `骑着马` | `qizhaoma` | `qizhema` | `zhe`→`zhao` |
| 11268 | `骑着马` | `qizhuoma` | `qizhema` | `zhe`→`zhuo` |
| 11008 | `避着` | `bizhao` | `bizhe` | `zhe`→`zhao` |
| 11008 | `避着` | `bizhuo` | `bizhe` | `zhe`→`zhuo` |
| 11006 | `潜藏着` | `qiancangzhao` | `qiancangzhe` | `zhe`→`zhao` |
| 11006 | `潜藏着` | `qiancangzhuo` | `qiancangzhe` | `zhe`→`zhuo` |
| 11006 | `手握着` | `shouwozhao` | `shouwozhe` | `zhe`→`zhao` |
| 11006 | `手握着` | `shouwozhuo` | `shouwozhe` | `zhe`→`zhuo` |
| 10888 | `飘着雪` | `piaozhaoxue` | `piaozhexue` | `zhe`→`zhao` |
| 10888 | `飘着雪` | `piaozhuoxue` | `piaozhexue` | `zhe`→`zhuo` |
| 10721 | `后不着店` | `houbuzhaodian` | `houbuzhedian` | `zhe`→`zhao` |
| 10721 | `后不着店` | `houbuzhuodian` | `houbuzhedian` | `zhe`→`zhuo` |
| 10721 | `抿着` | `minzhao` | `minzhe` | `zhe`→`zhao` |
| 10721 | `抿着` | `minzhuo` | `minzhe` | `zhe`→`zhuo` |
| 10721 | `腻着` | `nizhao` | `nizhe` | `zhe`→`zhao` |
| 10721 | `腻着` | `nizhuo` | `nizhe` | `zhe`→`zhuo` |
| 10634 | `点不着` | `dianbuzhao` | `dianbuzhe` | `zhe`→`zhao` |
| 10634 | `点不着` | `dianbuzhuo` | `dianbuzhe` | `zhe`→`zhuo` |
| 10543 | `暗恋着` | `anlianzhao` | `anlianzhe` | `zhe`→`zhao` |
| 10543 | `暗恋着` | `anlianzhuo` | `anlianzhe` | `zhe`→`zhuo` |
| 10543 | `见得着` | `jiandezhao` | `jiandezhe` | `zhe`→`zhao` |
| 10543 | `见得着` | `jiandezhuo` | `jiandezhe` | `zhe`→`zhuo` |
| 10539 | `不着痕迹` | `buzhaohenji` | `buzhehenji` | `zhe`→`zhao` |
| 10539 | `不着痕迹` | `buzhuohenji` | `buzhehenji` | `zhe`→`zhuo` |
| 10448 | `抱着书` | `baozhaoshu` | `baozheshu` | `zhe`→`zhao` |
| 10448 | `抱着书` | `baozhuoshu` | `baozheshu` | `zhe`→`zhuo` |
| 10448 | `闪着光` | `shanzhaoguang` | `shanzheguang` | `zhe`→`zhao` |
| 10448 | `闪着光` | `shanzhuoguang` | `shanzheguang` | `zhe`→`zhuo` |
| 10412 | `潜伏着` | `qianfuzhao` | `qianfuzhe` | `zhe`→`zhao` |
| 10412 | `潜伏着` | `qianfuzhuo` | `qianfuzhe` | `zhe`→`zhuo` |
| 10316 | `秉持着` | `bingchizhao` | `bingchizhe` | `zhe`→`zhao` |
| 10316 | `秉持着` | `bingchizhuo` | `bingchizhe` | `zhe`→`zhuo` |
| 10249 | `搀扶着` | `chanfuzhao` | `chanfuzhe` | `zhe`→`zhao` |
| 10249 | `搀扶着` | `chanfuzhuo` | `chanfuzhe` | `zhe`→`zhuo` |
| 10249 | `着作权` | `zhaozuoquan` | `zhezuoquan` | `zhe`→`zhao` |
| 10249 | `着作权` | `zhuozuoquan` | `zhezuoquan` | `zhe`→`zhuo` |
| 10234 | `瞄着` | `miaozhao` | `miaozhe` | `zhe`→`zhao` |
| 10234 | `瞄着` | `miaozhuo` | `miaozhe` | `zhe`→`zhuo` |
| 10234 | `梳着` | `shuzhao` | `shuzhe` | `zhe`→`zhao` |
| 10234 | `梳着` | `shuzhuo` | `shuzhe` | `zhe`→`zhuo` |
| 10142 | `绷着脸` | `bengzhaolian` | `bengzhelian` | `zhe`→`zhao` |
| 10142 | `绷着脸` | `bengzhuolian` | `bengzhelian` | `zhe`→`zhuo` |
| 10142 | `惦念着` | `diannianzhao` | `diannianzhe` | `zhe`→`zhao` |
| 10142 | `惦念着` | `diannianzhuo` | `diannianzhe` | `zhe`→`zhuo` |
| 10142 | `皱着眉头` | `zhouzhaomeitou` | `zhouzhemeitou` | `zhe`→`zhao` |
| 10142 | `皱着眉头` | `zhouzhuomeitou` | `zhouzhemeitou` | `zhe`→`zhuo` |
| 10079 | `捆着` | `kunzhao` | `kunzhe` | `zhe`→`zhao` |
| 10079 | `捆着` | `kunzhuo` | `kunzhe` | `zhe`→`zhuo` |
| 10000 | `凝视着` | `ningshizhao` | `ningshizhe` | `zhe`→`zhao` |
| 10000 | `凝视着` | `ningshizhuo` | `ningshizhe` | `zhe`→`zhuo` |
| 9916 | `夸着` | `kuazhao` | `kuazhe` | `zhe`→`zhao` |
| 9916 | `夸着` | `kuazhuo` | `kuazhe` | `zhe`→`zhuo` |
| 9916 | `抿着嘴` | `minzhaozui` | `minzhezui` | `zhe`→`zhao` |
| 9916 | `抿着嘴` | `minzhuozui` | `minzhezui` | `zhe`→`zhuo` |
| 9916 | `咬着不放` | `yaozhaobufang` | `yaozhebufang` | `zhe`→`zhao` |
| 9916 | `咬着不放` | `yaozhuobufang` | `yaozhebufang` | `zhe`→`zhuo` |
| 9864 | `紧握着` | `jinwozhao` | `jinwozhe` | `zhe`→`zhao` |
| 9864 | `紧握着` | `jinwozhuo` | `jinwozhe` | `zhe`→`zhuo` |
| 9795 | `臭名昭着` | `choumingzhaozhao` | `choumingzhaozhe` | `zhe`→`zhao` |
| 9795 | `臭名昭着` | `choumingzhaozhuo` | `choumingzhaozhe` | `zhe`→`zhuo` |
| 9795 | `着劲儿` | `zhaojiner` | `zhejiner` | `zhe`→`zhao` |
| 9795 | `着劲儿` | `zhuojiner` | `zhejiner` | `zhe`→`zhuo` |
| 9795 | `昭着` | `zhaozhao` | `zhaozhe` | `zhe`→`zhao` |
| 9795 | `昭着` | `zhaozhuo` | `zhaozhe` | `zhe`→`zhuo` |
| 9667 | `批着` | `pizhao` | `pizhe` | `zhe`→`zhao` |
| 9667 | `批着` | `pizhuo` | `pizhe` | `zhe`→`zhuo` |
| 9667 | `偎着` | `weizhao` | `weizhe` | `zhe`→`zhao` |
| 9667 | `偎着` | `weizhuo` | `weizhe` | `zhe`→`zhuo` |
| 9392 | `渴着` | `kezhao` | `kezhe` | `zhe`→`zhao` |
| 9392 | `渴着` | `kezhuo` | `kezhe` | `zhe`→`zhuo` |
| 9279 | `扳着` | `banzhao` | `banzhe` | `zhe`→`zhao` |
| 9279 | `扳着` | `banzhuo` | `banzhe` | `zhe`→`zhuo` |
| 9242 | `专着` | `zhuanzhao` | `zhuanzhe` | `zhe`→`zhao` |
| 9242 | `专着` | `zhuanzhuo` | `zhuanzhe` | `zhe`→`zhuo` |
| 9064 | `放着不管` | `fangzhaobuguan` | `fangzhebuguan` | `zhe`→`zhao` |
| 9064 | `放着不管` | `fangzhuobuguan` | `fangzhebuguan` | `zhe`→`zhuo` |
| 8990 | `镶着` | `xiangzhao` | `xiangzhe` | `zhe`→`zhao` |
| 8990 | `镶着` | `xiangzhuo` | `xiangzhe` | `zhe`→`zhuo` |
| 8913 | `惦着` | `dianzhao` | `dianzhe` | `zhe`→`zhao` |
| 8913 | `惦着` | `dianzhuo` | `dianzhe` | `zhe`→`zhuo` |
| 8771 | `题着` | `tizhao` | `tizhe` | `zhe`→`zhao` |
| 8771 | `题着` | `tizhuo` | `tizhe` | `zhe`→`zhuo` |
| 8769 | `点缀着` | `dianzhuizhao` | `dianzhuizhe` | `zhe`→`zhao` |
| 8769 | `点缀着` | `dianzhuizhuo` | `dianzhuizhe` | `zhe`→`zhuo` |
| 8769 | `蕴涵着` | `yunhanzhao` | `yunhanzhe` | `zhe`→`zhao` |
| 8769 | `蕴涵着` | `yunhanzhuo` | `yunhanzhe` | `zhe`→`zhuo` |
| 8723 | `挟着` | `xiezhao` | `xiezhe` | `zhe`→`zhao` |
| 8723 | `挟着` | `xiezhuo` | `xiezhe` | `zhe`→`zhuo` |
| 8535 | `闷着头` | `menzhaotou` | `menzhetou` | `zhe`→`zhao` |
| 8535 | `闷着头` | `menzhuotou` | `menzhetou` | `zhe`→`zhuo` |
| 8535 | `生着气` | `shengzhaoqi` | `shengzheqi` | `zhe`→`zhao` |
| 8535 | `生着气` | `shengzhuoqi` | `shengzheqi` | `zhe`→`zhuo` |
| 8535 | `找得着` | `zhaodezhao` | `zhaodezhe` | `zhe`→`zhao` |
| 8535 | `找得着` | `zhaodezhuo` | `zhaodezhe` | `zhe`→`zhuo` |
| 8393 | `簇拥着` | `cuyongzhao` | `cuyongzhe` | `zhe`→`zhao` |
| 8393 | `簇拥着` | `cuyongzhuo` | `cuyongzhe` | `zhe`→`zhuo` |
| 8354 | `着者` | `zhaozhe` | `zhezhe` | `zhe`→`zhao` |
| 8354 | `着者` | `zhuozhe` | `zhezhe` | `zhe`→`zhuo` |
| 8325 | `混杂着` | `hunzazhao` | `hunzazhe` | `zhe`→`zhao` |
| 8325 | `混杂着` | `hunzazhuo` | `hunzazhe` | `zhe`→`zhuo` |
| 8323 | `趴着不动` | `pazhaobudong` | `pazhebudong` | `zhe`→`zhao` |
| 8323 | `趴着不动` | `pazhuobudong` | `pazhebudong` | `zhe`→`zhuo` |
| 8323 | `签着` | `qianzhao` | `qianzhe` | `zhe`→`zhao` |
| 8323 | `签着` | `qianzhuo` | `qianzhe` | `zhe`→`zhuo` |
| 8323 | `相着` | `xiangzhao` | `xiangzhe` | `zhe`→`zhao` |
| 8323 | `相着` | `xiangzhuo` | `xiangzhe` | `zhe`→`zhuo` |
| 8323 | `正开着` | `zhengkaizhao` | `zhengkaizhe` | `zhe`→`zhao` |
| 8323 | `正开着` | `zhengkaizhuo` | `zhengkaizhe` | `zhe`→`zhuo` |
| 8097 | `缠绕着` | `chanraozhao` | `chanraozhe` | `zhe`→`zhao` |
| 8097 | `缠绕着` | `chanraozhuo` | `chanraozhe` | `zhe`→`zhuo` |
| 8091 | `见微知着` | `jianweizhizhao` | `jianweizhizhe` | `zhe`→`zhao` |
| 8091 | `见微知着` | `jianweizhizhuo` | `jianweizhizhe` | `zhe`→`zhuo` |
| 7836 | `驼着背` | `tuozhaobei` | `tuozhebei` | `zhe`→`zhao` |
| 7836 | `驼着背` | `tuozhuobei` | `tuozhebei` | `zhe`→`zhuo` |
| 7836 | `掩盖着` | `yangaizhao` | `yangaizhe` | `zhe`→`zhao` |
| 7836 | `掩盖着` | `yangaizhuo` | `yangaizhe` | `zhe`→`zhuo` |
| 7564 | `谈论着` | `tanlunzhao` | `tanlunzhe` | `zhe`→`zhao` |
| 7564 | `谈论着` | `tanlunzhuo` | `tanlunzhe` | `zhe`→`zhuo` |
| 7552 | `交叉着` | `jiaochazhao` | `jiaochazhe` | `zhe`→`zhao` |
| 7552 | `交叉着` | `jiaochazhuo` | `jiaochazhe` | `zhe`→`zhuo` |
| 7552 | `紧绷着` | `jinbengzhao` | `jinbengzhe` | `zhe`→`zhao` |
| 7552 | `紧绷着` | `jinbengzhuo` | `jinbengzhe` | `zhe`→`zhuo` |
| 7552 | `巨着` | `juzhao` | `juzhe` | `zhe`→`zhao` |
| 7552 | `巨着` | `juzhuo` | `juzhe` | `zhe`→`zhuo` |
| 7552 | `囔着` | `nangzhao` | `nangzhe` | `zhe`→`zhao` |
| 7552 | `囔着` | `nangzhuo` | `nangzhe` | `zhe`→`zhuo` |
| 7552 | `掩藏着` | `yancangzhao` | `yancangzhe` | `zhe`→`zhao` |
| 7552 | `掩藏着` | `yancangzhuo` | `yancangzhe` | `zhe`→`zhuo` |
| 7312 | `一着不慎` | `yizhaobushen` | `yizhebushen` | `zhe`→`zhao` |
| 7312 | `一着不慎` | `yizhuobushen` | `yizhebushen` | `zhe`→`zhuo` |
| 7233 | `放着不用` | `fangzhaobuyong` | `fangzhebuyong` | `zhe`→`zhao` |
| 7233 | `放着不用` | `fangzhuobuyong` | `fangzhebuyong` | `zhe`→`zhuo` |
| 7233 | `滑着走` | `huazhaozou` | `huazhezou` | `zhe`→`zhao` |
| 7233 | `滑着走` | `huazhuozou` | `huazhezou` | `zhe`→`zhuo` |
| 7233 | `想过着` | `xiangguozhao` | `xiangguozhe` | `zhe`→`zhao` |
| 7233 | `想过着` | `xiangguozhuo` | `xiangguozhe` | `zhe`→`zhuo` |
| 6868 | `梗着` | `gengzhao` | `gengzhe` | `zhe`→`zhao` |
| 6868 | `梗着` | `gengzhuo` | `gengzhe` | `zhe`→`zhuo` |
| 6868 | `眷恋着` | `juanlianzhao` | `juanlianzhe` | `zhe`→`zhao` |
| 6868 | `眷恋着` | `juanlianzhuo` | `juanlianzhe` | `zhe`→`zhuo` |
| 6868 | `没着没落` | `meizhaomoluo` | `meizhemoluo` | `zhe`→`zhao` |
| 6868 | `没着没落` | `meizhuomoluo` | `meizhemoluo` | `zhe`→`zhuo` |
| 6868 | `拍打着` | `paidazhao` | `paidazhe` | `zhe`→`zhao` |
| 6868 | `拍打着` | `paidazhuo` | `paidazhe` | `zhe`→`zhuo` |
| 6834 | `高举着` | `gaojuzhao` | `gaojuzhe` | `zhe`→`zhao` |
| 6834 | `高举着` | `gaojuzhuo` | `gaojuzhe` | `zhe`→`zhuo` |
| 6722 | `扶持着` | `fuchizhao` | `fuchizhe` | `zhe`→`zhao` |
| 6722 | `扶持着` | `fuchizhuo` | `fuchizhe` | `zhe`→`zhuo` |
| 6528 | `舞动着` | `wudongzhao` | `wudongzhe` | `zhe`→`zhao` |
| 6528 | `舞动着` | `wudongzhuo` | `wudongzhe` | `zhe`→`zhuo` |
| 6516 | `摆放着` | `baifangzhao` | `baifangzhe` | `zhe`→`zhao` |
| 6516 | `摆放着` | `baifangzhuo` | `baifangzhe` | `zhe`→`zhuo` |
| 6463 | `追寻着` | `zhuixunzhao` | `zhuixunzhe` | `zhe`→`zhao` |
| 6463 | `追寻着` | `zhuixunzhuo` | `zhuixunzhe` | `zhe`→`zhuo` |
| 6442 | `炫耀着` | `xuanyaozhao` | `xuanyaozhe` | `zhe`→`zhao` |
| 6442 | `炫耀着` | `xuanyaozhuo` | `xuanyaozhe` | `zhe`→`zhuo` |
| 6442 | `摇着头` | `yaozhaotou` | `yaozhetou` | `zhe`→`zhao` |
| 6442 | `摇着头` | `yaozhuotou` | `yaozhetou` | `zhe`→`zhuo` |
| 6442 | `隐忍着` | `yinrenzhao` | `yinrenzhe` | `zhe`→`zhao` |
| 6442 | `隐忍着` | `yinrenzhuo` | `yinrenzhe` | `zhe`→`zhuo` |
| 6380 | `擎着` | `qingzhao` | `qingzhe` | `zhe`→`zhao` |
| 6380 | `擎着` | `qingzhuo` | `qingzhe` | `zhe`→`zhuo` |
| 6235 | `玩弄着` | `wannongzhao` | `wannongzhe` | `zhe`→`zhao` |
| 6235 | `玩弄着` | `wannongzhuo` | `wannongzhe` | `zhe`→`zhuo` |
| 6134 | `弹着点` | `danzhaodian` | `danzhedian` | `zhe`→`zhao` |
| 6134 | `弹着点` | `danzhuodian` | `danzhedian` | `zhe`→`zhuo` |
| 6044 | `所着` | `suozhao` | `suozhe` | `zhe`→`zhao` |
| 6044 | `所着` | `suozhuo` | `suozhe` | `zhe`→`zhuo` |
| 5930 | `爱恋着` | `ailianzhao` | `ailianzhe` | `zhe`→`zhao` |
| 5930 | `爱恋着` | `ailianzhuo` | `ailianzhe` | `zhe`→`zhuo` |
| 5930 | `杠着` | `gangzhao` | `gangzhe` | `zhe`→`zhao` |
| 5930 | `杠着` | `gangzhuo` | `gangzhe` | `zhe`→`zhuo` |
| 5930 | `描着` | `miaozhao` | `miaozhe` | `zhe`→`zhao` |
| 5930 | `描着` | `miaozhuo` | `miaozhe` | `zhe`→`zhuo` |
| 5930 | `迷恋着` | `milianzhao` | `milianzhe` | `zhe`→`zhao` |
| 5930 | `迷恋着` | `milianzhuo` | `milianzhe` | `zhe`→`zhuo` |
| 5930 | `手夹着` | `shoujiazhao` | `shoujiazhe` | `zhe`→`zhao` |
| 5930 | `手夹着` | `shoujiazhuo` | `shoujiazhe` | `zhe`→`zhuo` |
| 5686 | `交织着` | `jiaozhizhao` | `jiaozhizhe` | `zhe`→`zhao` |
| 5686 | `交织着` | `jiaozhizhuo` | `jiaozhizhe` | `zhe`→`zhuo` |
| 5686 | `着丝粒` | `zhaosili` | `zhesili` | `zhe`→`zhao` |
| 5686 | `着丝粒` | `zhuosili` | `zhesili` | `zhe`→`zhuo` |
| 5633 | `抱持着` | `baochizhao` | `baochizhe` | `zhe`→`zhao` |
| 5633 | `抱持着` | `baochizhuo` | `baochizhe` | `zhe`→`zhuo` |
| 5633 | `围坐着` | `weizuozhao` | `weizuozhe` | `zhe`→`zhao` |
| 5633 | `围坐着` | `weizuozhuo` | `weizuozhe` | `zhe`→`zhuo` |
| 5286 | `扮着` | `banzhao` | `banzhe` | `zhe`→`zhao` |
| 5286 | `扮着` | `banzhuo` | `banzhe` | `zhe`→`zhuo` |
| 5286 | `盛开着` | `shengkaizhao` | `shengkaizhe` | `zhe`→`zhao` |
| 5286 | `盛开着` | `shengkaizhuo` | `shengkaizhe` | `zhe`→`zhuo` |
| 5286 | `挂记着` | `guajizhao` | `guajizhe` | `zhe`→`zhao` |
| 5286 | `挂记着` | `guajizhuo` | `guajizhe` | `zhe`→`zhuo` |
| 5286 | `践踏着` | `jiantazhao` | `jiantazhe` | `zhe`→`zhao` |
| 5286 | `践踏着` | `jiantazhuo` | `jiantazhe` | `zhe`→`zhuo` |
| 5286 | `攀着` | `panzhao` | `panzhe` | `zhe`→`zhao` |
| 5286 | `攀着` | `panzhuo` | `panzhe` | `zhe`→`zhuo` |
| 5286 | `屏蔽着` | `pingbizhao` | `pingbizhe` | `zhe`→`zhao` |
| 5286 | `屏蔽着` | `pingbizhuo` | `pingbizhe` | `zhe`→`zhuo` |
| 5286 | `偷鸡不着` | `toujibuzhao` | `toujibuzhe` | `zhe`→`zhao` |
| 5286 | `偷鸡不着` | `toujibuzhuo` | `toujibuzhe` | `zhe`→`zhuo` |
| 5286 | `凸着` | `tuzhao` | `tuzhe` | `zhe`→`zhao` |
| 5286 | `凸着` | `tuzhuo` | `tuzhe` | `zhe`→`zhuo` |
| 5286 | `着作等身` | `zhaozuodengshen` | `zhezuodengshen` | `zhe`→`zhao` |
| 5286 | `着作等身` | `zhuozuodengshen` | `zhezuodengshen` | `zhe`→`zhuo` |
| 5072 | `穿插着` | `chuanchazhao` | `chuanchazhe` | `zhe`→`zhao` |
| 5072 | `穿插着` | `chuanchazhuo` | `chuanchazhe` | `zhe`→`zhuo` |
| 4949 | `覆着` | `fuzhao` | `fuzhe` | `zhe`→`zhao` |
| 4949 | `覆着` | `fuzhuo` | `fuzhe` | `zhe`→`zhuo` |
| 4652 | `手持着` | `shouchizhao` | `shouchizhe` | `zhe`→`zhao` |
| 4652 | `手持着` | `shouchizhuo` | `shouchizhe` | `zhe`→`zhuo` |
| 4640 | `耸立着` | `songlizhao` | `songlizhe` | `zhe`→`zhao` |
| 4640 | `耸立着` | `songlizhuo` | `songlizhe` | `zhe`→`zhuo` |
| 4422 | `好多着` | `haoduozhao` | `haoduozhe` | `zhe`→`zhao` |
| 4422 | `好多着` | `haoduozhuo` | `haoduozhe` | `zhe`→`zhuo` |
| 4422 | `努着` | `nuzhao` | `nuzhe` | `zhe`→`zhao` |
| 4422 | `努着` | `nuzhuo` | `nuzhe` | `zhe`→`zhuo` |
| 4422 | `贪恋着` | `tanlianzhao` | `tanlianzhe` | `zhe`→`zhao` |
| 4422 | `贪恋着` | `tanlianzhuo` | `tanlianzhe` | `zhe`→`zhuo` |
| 4422 | `突着` | `tuzhao` | `tuzhe` | `zhe`→`zhao` |
| 4422 | `突着` | `tuzhuo` | `tuzhe` | `zhe`→`zhuo` |
| 4422 | `仰仗着` | `yangzhangzhao` | `yangzhangzhe` | `zhe`→`zhao` |
| 4422 | `仰仗着` | `yangzhangzhuo` | `yangzhangzhe` | `zhe`→`zhuo` |
| 4422 | `着名人物` | `zhaomingrenwu` | `zhemingrenwu` | `zhe`→`zhao` |
| 4422 | `着名人物` | `zhuomingrenwu` | `zhemingrenwu` | `zhe`→`zhuo` |
| 4422 | `卓着` | `zhuozhao` | `zhuozhe` | `zhe`→`zhao` |
| 4422 | `卓着` | `zhuozhuo` | `zhuozhe` | `zhe`→`zhuo` |
| 4193 | `紧靠着` | `jinkaozhao` | `jinkaozhe` | `zhe`→`zhao` |
| 4193 | `紧靠着` | `jinkaozhuo` | `jinkaozhe` | `zhe`→`zhuo` |
| 4131 | `掩映着` | `yanyingzhao` | `yanyingzhe` | `zhe`→`zhao` |
| 4131 | `掩映着` | `yanyingzhuo` | `yanyingzhe` | `zhe`→`zhuo` |
| 4131 | `着恼` | `zhaonao` | `zhenao` | `zhe`→`zhao` |
| 4131 | `着恼` | `zhuonao` | `zhenao` | `zhe`→`zhuo` |
| 3861 | `着丝点` | `zhaosidian` | `zhesidian` | `zhe`→`zhao` |
| 3861 | `着丝点` | `zhuosidian` | `zhesidian` | `zhe`→`zhuo` |
| 3831 | `张开着` | `zhangkaizhao` | `zhangkaizhe` | `zhe`→`zhao` |
| 3831 | `张开着` | `zhangkaizhuo` | `zhangkaizhe` | `zhe`→`zhuo` |
| 3686 | `并存着` | `bingcunzhao` | `bingcunzhe` | `zhe`→`zhao` |
| 3686 | `并存着` | `bingcunzhuo` | `bingcunzhe` | `zhe`→`zhuo` |
| 3686 | `弹奏着` | `danzouzhao` | `danzouzhe` | `zhe`→`zhao` |
| 3686 | `弹奏着` | `danzouzhuo` | `danzouzhe` | `zhe`→`zhuo` |
| 3686 | `紧邻着` | `jinlinzhao` | `jinlinzhe` | `zhe`→`zhao` |
| 3686 | `紧邻着` | `jinlinzhuo` | `jinlinzhe` | `zhe`→`zhuo` |
| 3686 | `遗着` | `yizhao` | `yizhe` | `zhe`→`zhao` |
| 3686 | `遗着` | `yizhuo` | `yizhe` | `zhe`→`zhuo` |
| 3239 | `眯着` | `mizhao` | `mizhe` | `zhe`→`zhao` |
| 3239 | `眯着` | `mizhuo` | `mizhe` | `zhe`→`zhuo` |
| 3239 | `凭靠着` | `pingkaozhao` | `pingkaozhe` | `zhe`→`zhao` |
| 3239 | `凭靠着` | `pingkaozhuo` | `pingkaozhe` | `zhe`→`zhuo` |
| 3239 | `遮盖着` | `zhegaizhao` | `zhegaizhe` | `zhe`→`zhao` |
| 3239 | `遮盖着` | `zhegaizhuo` | `zhegaizhe` | `zhe`→`zhuo` |
| 3094 | `百着` | `baizhao` | `baizhe` | `zhe`→`zhao` |
| 3094 | `百着` | `baizhuo` | `baizhe` | `zhe`→`zhuo` |
| 3094 | `惦挂着` | `dianguazhao` | `dianguazhe` | `zhe`→`zhao` |
| 3094 | `惦挂着` | `dianguazhuo` | `dianguazhe` | `zhe`→`zhuo` |
| 3094 | `躲藏着` | `duocangzhao` | `duocangzhe` | `zhe`→`zhao` |
| 3094 | `躲藏着` | `duocangzhuo` | `duocangzhe` | `zhe`→`zhuo` |
| 3094 | `哽着` | `gengzhao` | `gengzhe` | `zhe`→`zhao` |
| 3094 | `哽着` | `gengzhuo` | `gengzhe` | `zhe`→`zhuo` |
| 3094 | `焊着` | `hanzhao` | `hanzhe` | `zhe`→`zhao` |
| 3094 | `焊着` | `hanzhuo` | `hanzhe` | `zhe`→`zhuo` |
| 3094 | `几着` | `jizhao` | `jizhe` | `zhe`→`zhao` |
| 3094 | `几着` | `jizhuo` | `jizhe` | `zhe`→`zhuo` |
| 3094 | `两着` | `liangzhao` | `liangzhe` | `zhe`→`zhao` |
| 3094 | `两着` | `liangzhuo` | `liangzhe` | `zhe`→`zhuo` |
| 3094 | `留心看着` | `liuxinkanzhao` | `liuxinkanzhe` | `zhe`→`zhao` |
| 3094 | `留心看着` | `liuxinkanzhuo` | `liuxinkanzhe` | `zhe`→`zhuo` |
| 3094 | `佩着` | `peizhao` | `peizhe` | `zhe`→`zhao` |
| 3094 | `佩着` | `peizhuo` | `peizhe` | `zhe`→`zhuo` |
| 3094 | `歉着` | `qianzhao` | `qianzhe` | `zhe`→`zhao` |
| 3094 | `歉着` | `qianzhuo` | `qianzhe` | `zhe`→`zhuo` |
| 3094 | `首着` | `shouzhao` | `shouzhe` | `zhe`→`zhao` |
| 3094 | `首着` | `shouzhuo` | `shouzhe` | `zhe`→`zhuo` |
| 3094 | `剔着` | `tizhao` | `tizhe` | `zhe`→`zhao` |
| 3094 | `剔着` | `tizhuo` | `tizhe` | `zhe`→`zhuo` |
| 3094 | `围看着` | `weikanzhao` | `weikanzhe` | `zhe`→`zhao` |
| 3094 | `围看着` | `weikanzhuo` | `weikanzhe` | `zhe`→`zhuo` |
| 3094 | `消着` | `xiaozhao` | `xiaozhe` | `zhe`→`zhao` |
| 3094 | `消着` | `xiaozhuo` | `xiaozhe` | `zhe`→`zhuo` |
| 3094 | `仰躺着` | `yangtangzhao` | `yangtangzhe` | `zhe`→`zhao` |
| 3094 | `仰躺着` | `yangtangzhuo` | `yangtangzhe` | `zhe`→`zhuo` |
| 3094 | `遮蔽着` | `zhebizhao` | `zhebizhe` | `zhe`→`zhao` |
| 3094 | `遮蔽着` | `zhebizhuo` | `zhebizhe` | `zhe`→`zhuo` |
| 3094 | `阻拦着` | `zulanzhao` | `zulanzhe` | `zhe`→`zhao` |
| 3094 | `阻拦着` | `zulanzhuo` | `zulanzhe` | `zhe`→`zhuo` |
| 1493 | `三十六着` | `sanshiliuzhao` | `sanshiliuzhe` | `zhe`→`zhao` |
| 1493 | `三十六着` | `sanshiliuzhuo` | `sanshiliuzhe` | `zhe`→`zhuo` |
| 0 | `暗算着` | `ansuanzhao` | `ansuanzhe` | `zhe`→`zhao` |
| 0 | `暗算着` | `ansuanzhuo` | `ansuanzhe` | `zhe`→`zhuo` |
| 0 | `摆列着` | `bailiezhao` | `bailiezhe` | `zhe`→`zhao` |
| 0 | `摆列着` | `bailiezhuo` | `bailiezhe` | `zhe`→`zhuo` |
| 0 | `抱着理想` | `baozhaolixiang` | `baozhelixiang` | `zhe`→`zhao` |
| 0 | `抱着理想` | `baozhuolixiang` | `baozhelixiang` | `zhe`→`zhuo` |
| 0 | `被覆着` | `beifuzhao` | `beifuzhe` | `zhe`→`zhao` |
| 0 | `被覆着` | `beifuzhuo` | `beifuzhe` | `zhe`→`zhuo` |
| 0 | `背诵着` | `beisongzhao` | `beisongzhe` | `zhe`→`zhao` |
| 0 | `背诵着` | `beisongzhuo` | `beisongzhe` | `zhe`→`zhuo` |
| 0 | `背向着` | `beixiangzhao` | `beixiangzhe` | `zhe`→`zhao` |
| 0 | `背向着` | `beixiangzhuo` | `beixiangzhe` | `zhe`→`zhuo` |
| 0 | `绷着劲` | `bengzhaojin` | `bengzhejin` | `zhe`→`zhao` |
| 0 | `绷着劲` | `bengzhuojin` | `bengzhejin` | `zhe`→`zhuo` |
| 0 | `绷着脸儿` | `bengzhaolianer` | `bengzhelianer` | `zhe`→`zhao` |
| 0 | `绷着脸儿` | `bengzhuolianer` | `bengzhelianer` | `zhe`→`zhuo` |
| 0 | `标榜着` | `biaobangzhao` | `biaobangzhe` | `zhe`→`zhao` |
| 0 | `标榜着` | `biaobangzhuo` | `biaobangzhe` | `zhe`→`zhuo` |
| 0 | `笔名着作` | `bimingzhaozuo` | `bimingzhezuo` | `zhe`→`zhao` |
| 0 | `笔名着作` | `bimingzhuozuo` | `bimingzhezuo` | `zhe`→`zhuo` |
| 0 | `禀着` | `bingzhao` | `bingzhe` | `zhe`→`zhao` |
| 0 | `禀着` | `bingzhuo` | `bingzhe` | `zhe`→`zhuo` |
| 0 | `擦抹着` | `camozhao` | `camozhe` | `zhe`→`zhao` |
| 0 | `擦抹着` | `camozhuo` | `camozhe` | `zhe`→`zhuo` |
| 0 | `搀杂着` | `chanzazhao` | `chanzazhe` | `zhe`→`zhao` |
| 0 | `搀杂着` | `chanzazhuo` | `chanzazhe` | `zhe`→`zhuo` |
| 0 | `搽着` | `chazhao` | `chazhe` | `zhe`→`zhao` |
| 0 | `搽着` | `chazhuo` | `chazhe` | `zhe`→`zhuo` |
| 0 | `成效卓着` | `chengxiaozhuozhao` | `chengxiaozhuozhe` | `zhe`→`zhao` |
| 0 | `成效卓着` | `chengxiaozhuozhuo` | `chengxiaozhuozhe` | `zhe`→`zhuo` |
| 0 | `承袭着` | `chengxizhao` | `chengxizhe` | `zhe`→`zhao` |
| 0 | `承袭着` | `chengxizhuo` | `chengxizhe` | `zhe`→`zhuo` |
| 0 | `充塞着` | `chongsezhao` | `chongsezhe` | `zhe`→`zhao` |
| 0 | `充塞着` | `chongsezhuo` | `chongsezhe` | `zhe`→`zhuo` |
| 0 | `错杂着` | `cuozazhao` | `cuozazhe` | `zhe`→`zhao` |
| 0 | `错杂着` | `cuozazhuo` | `cuozazhe` | `zhe`→`zhuo` |
| 0 | `打持着` | `dachizhao` | `dachizhe` | `zhe`→`zhao` |
| 0 | `打持着` | `dachizhuo` | `dachizhe` | `zhe`→`zhuo` |
| 0 | `单靠着` | `dankaozhao` | `dankaozhe` | `zhe`→`zhao` |
| 0 | `单靠着` | `dankaozhuo` | `dankaozhe` | `zhe`→`zhuo` |
| 0 | `吊挂着` | `diaoguazhao` | `diaoguazhe` | `zhe`→`zhao` |
| 0 | `吊挂着` | `diaoguazhuo` | `diaoguazhe` | `zhe`→`zhuo` |
| 0 | `抖动着` | `doudongzhao` | `doudongzhe` | `zhe`→`zhao` |
| 0 | `抖动着` | `doudongzhuo` | `doudongzhe` | `zhe`→`zhuo` |
| 0 | `多着丝` | `duozhaosi` | `duozhesi` | `zhe`→`zhao` |
| 0 | `多着丝` | `duozhuosi` | `duozhesi` | `zhe`→`zhuo` |
| 0 | `耳塞着` | `ersaizhao` | `ersaizhe` | `zhe`→`zhao` |
| 0 | `耳塞着` | `ersaizhuo` | `ersaizhe` | `zhe`→`zhuo` |
| 0 | `非单着丝` | `feidanzhaosi` | `feidanzhesi` | `zhe`→`zhao` |
| 0 | `非单着丝` | `feidanzhuosi` | `feidanzhesi` | `zhe`→`zhuo` |
| 0 | `非端着丝` | `feiduanzhaosi` | `feiduanzhesi` | `zhe`→`zhao` |
| 0 | `非端着丝` | `feiduanzhuosi` | `feiduanzhesi` | `zhe`→`zhuo` |
| 0 | `搁着不动` | `gezhaobudong` | `gezhebudong` | `zhe`→`zhao` |
| 0 | `搁着不动` | `gezhuobudong` | `gezhebudong` | `zhe`→`zhuo` |
| 0 | `搁着不管` | `gezhaobuguan` | `gezhebuguan` | `zhe`→`zhao` |
| 0 | `搁着不管` | `gezhuobuguan` | `gezhebuguan` | `zhe`→`zhuo` |
| 0 | `搆不着` | `goubuzhao` | `goubuzhe` | `zhe`→`zhao` |
| 0 | `搆不着` | `goubuzhuo` | `goubuzhe` | `zhe`→`zhuo` |
| 0 | `颳着` | `guazhao` | `guazhe` | `zhe`→`zhao` |
| 0 | `颳着` | `guazhuo` | `guazhe` | `zhe`→`zhuo` |
| 0 | `合不着` | `hebuzhao` | `hebuzhe` | `zhe`→`zhao` |
| 0 | `合不着` | `hebuzhuo` | `hebuzhe` | `zhe`→`zhuo` |
| 0 | `皇皇巨着` | `huanghuangjuzhao` | `huanghuangjuzhe` | `zhe`→`zhao` |
| 0 | `皇皇巨着` | `huanghuangjuzhuo` | `huanghuangjuzhe` | `zhe`→`zhuo` |
| 0 | `间杂着` | `jianzazhao` | `jianzazhe` | `zhe`→`zhao` |
| 0 | `间杂着` | `jianzazhuo` | `jianzazhe` | `zhe`→`zhuo` |
| 0 | `记挂着` | `jiguazhao` | `jiguazhe` | `zhe`→`zhao` |
| 0 | `记挂着` | `jiguazhuo` | `jiguazhe` | `zhe`→`zhuo` |
| 0 | `紧逼着` | `jinbizhao` | `jinbizhe` | `zhe`→`zhao` |
| 0 | `紧逼着` | `jinbizhuo` | `jinbizhe` | `zhe`→`zhuo` |
| 0 | `紧缩着` | `jinsuozhao` | `jinsuozhe` | `zhe`→`zhao` |
| 0 | `紧缩着` | `jinsuozhuo` | `jinsuozhe` | `zhe`→`zhuo` |
| 0 | `刊着` | `kanzhao` | `kanzhe` | `zhe`→`zhao` |
| 0 | `刊着` | `kanzhuo` | `kanzhe` | `zhe`→`zhuo` |
| 0 | `渴盼着` | `kepanzhao` | `kepanzhe` | `zhe`→`zhao` |
| 0 | `渴盼着` | `kepanzhuo` | `kepanzhe` | `zhe`→`zhuo` |
| 0 | `口喊着` | `kouhanzhao` | `kouhanzhe` | `zhe`→`zhao` |
| 0 | `口喊着` | `kouhanzhuo` | `kouhanzhe` | `zhe`→`zhuo` |
| 0 | `釦着` | `kouzhao` | `kouzhe` | `zhe`→`zhao` |
| 0 | `釦着` | `kouzhuo` | `kouzhe` | `zhe`→`zhuo` |
| 0 | `框着` | `kuangzhao` | `kuangzhe` | `zhe`→`zhao` |
| 0 | `框着` | `kuangzhuo` | `kuangzhe` | `zhe`→`zhuo` |
| 0 | `临靠着` | `linkaozhao` | `linkaozhe` | `zhe`→`zhao` |
| 0 | `临靠着` | `linkaozhuo` | `linkaozhe` | `zhe`→`zhuo` |
| 0 | `轮着` | `lunzhao` | `lunzhe` | `zhe`→`zhao` |
| 0 | `轮着` | `lunzhuo` | `lunzhe` | `zhe`→`zhuo` |
| 0 | `贸着之仇` | `maozhaozhichou` | `maozhezhichou` | `zhe`→`zhao` |
| 0 | `贸着之仇` | `maozhuozhichou` | `maozhezhichou` | `zhe`→`zhuo` |
| 0 | `面朝着` | `mianchaozhao` | `mianchaozhe` | `zhe`→`zhao` |
| 0 | `面朝着` | `mianchaozhuo` | `mianchaozhe` | `zhe`→`zhuo` |
| 0 | `捏握着` | `niewozhao` | `niewozhe` | `zhe`→`zhao` |
| 0 | `捏握着` | `niewozhuo` | `niewozhe` | `zhe`→`zhuo` |
| 0 | `蹑着` | `niezhao` | `niezhe` | `zhe`→`zhao` |
| 0 | `蹑着` | `niezhuo` | `niezhe` | `zhe`→`zhuo` |
| 0 | `蹑着脚` | `niezhaojiao` | `niezhejiao` | `zhe`→`zhao` |
| 0 | `蹑着脚` | `niezhuojiao` | `niezhejiao` | `zhe`→`zhuo` |
| 0 | `盘桓着` | `panhuanzhao` | `panhuanzhe` | `zhe`→`zhao` |
| 0 | `盘桓着` | `panhuanzhuo` | `panhuanzhe` | `zhe`→`zhuo` |
| 0 | `盘绕着` | `panraozhao` | `panraozhe` | `zhe`→`zhao` |
| 0 | `盘绕着` | `panraozhuo` | `panraozhe` | `zhe`→`zhuo` |
| 0 | `盘坐着` | `panzuozhao` | `panzuozhe` | `zhe`→`zhao` |
| 0 | `盘坐着` | `panzuozhuo` | `panzuozhe` | `zhe`→`zhuo` |
| 0 | `佩带着` | `peidaizhao` | `peidaizhe` | `zhe`→`zhao` |
| 0 | `佩带着` | `peidaizhuo` | `peidaizhe` | `zhe`→`zhuo` |
| 0 | `喷洒着` | `pensazhao` | `pensazhe` | `zhe`→`zhao` |
| 0 | `喷洒着` | `pensazhuo` | `pensazhe` | `zhe`→`zhuo` |
| 0 | `喷着气` | `penzhaoqi` | `penzheqi` | `zhe`→`zhao` |
| 0 | `喷着气` | `penzhuoqi` | `penzheqi` | `zhe`→`zhuo` |
| 0 | `偏顾着` | `pianguzhao` | `pianguzhe` | `zhe`→`zhao` |
| 0 | `偏顾着` | `pianguzhuo` | `pianguzhe` | `zhe`→`zhuo` |
| 0 | `偏护着` | `pianhuzhao` | `pianhuzhe` | `zhe`→`zhao` |
| 0 | `偏护着` | `pianhuzhuo` | `pianhuzhe` | `zhe`→`zhuo` |
| 0 | `飘舞着` | `piaowuzhao` | `piaowuzhe` | `zhe`→`zhao` |
| 0 | `飘舞着` | `piaowuzhuo` | `piaowuzhe` | `zhe`→`zhuo` |
| 0 | `披盖着` | `pigaizhao` | `pigaizhe` | `zhe`→`zhao` |
| 0 | `披盖着` | `pigaizhuo` | `pigaizhe` | `zhe`→`zhuo` |
| 0 | `平卧着` | `pingwozhao` | `pingwozhe` | `zhe`→`zhao` |
| 0 | `平卧着` | `pingwozhuo` | `pingwozhe` | `zhe`→`zhuo` |
| 0 | `批示着` | `pishizhao` | `pishizhe` | `zhe`→`zhao` |
| 0 | `批示着` | `pishizhuo` | `pishizhe` | `zhe`→`zhuo` |
| 0 | `牵挂着` | `qianguazhao` | `qianguazhe` | `zhe`→`zhao` |
| 0 | `牵挂着` | `qianguazhuo` | `qianguazhe` | `zhe`→`zhuo` |
| 0 | `牵拉着` | `qianlazhao` | `qianlazhe` | `zhe`→`zhao` |
| 0 | `牵拉着` | `qianlazhuo` | `qianlazhe` | `zhe`→`zhuo` |
| 0 | `欠缺着` | `qianquezhao` | `qianquezhe` | `zhe`→`zhao` |
| 0 | `欠缺着` | `qianquezhuo` | `qianquezhe` | `zhe`→`zhuo` |
| 0 | `撬着` | `qiaozhao` | `qiaozhe` | `zhe`→`zhao` |
| 0 | `撬着` | `qiaozhuo` | `qiaozhe` | `zhe`→`zhuo` |
| 0 | `跷着` | `qiaozhao` | `qiaozhe` | `zhe`→`zhao` |
| 0 | `跷着` | `qiaozhuo` | `qiaozhe` | `zhe`→`zhuo` |
| 0 | `棋错一着` | `qicuoyizhao` | `qicuoyizhe` | `zhe`→`zhao` |
| 0 | `棋错一着` | `qicuoyizhuo` | `qicuoyizhe` | `zhe`→`zhuo` |
| 0 | `瑟缩着` | `sesuozhao` | `sesuozhe` | `zhe`→`zhao` |
| 0 | `瑟缩着` | `sesuozhuo` | `sesuozhe` | `zhe`→`zhuo` |
| 0 | `声誉卓着` | `shengyuzhuozhao` | `shengyuzhuozhe` | `zhe`→`zhao` |
| 0 | `声誉卓着` | `shengyuzhuozhuo` | `shengyuzhuozhe` | `zhe`→`zhuo` |
| 0 | `十余着` | `shiyuzhao` | `shiyuzhe` | `zhe`→`zhao` |
| 0 | `十余着` | `shiyuzhuo` | `shiyuzhe` | `zhe`→`zhuo` |
| 0 | `手包着` | `shoubaozhao` | `shoubaozhe` | `zhe`→`zhao` |
| 0 | `手包着` | `shoubaozhuo` | `shoubaozhe` | `zhe`→`zhuo` |
| 0 | `收存着` | `shoucunzhao` | `shoucunzhe` | `zhe`→`zhao` |
| 0 | `收存着` | `shoucunzhuo` | `shoucunzhe` | `zhe`→`zhuo` |
| 0 | `熟视着` | `shushizhao` | `shushizhe` | `zhe`→`zhao` |
| 0 | `熟视着` | `shushizhuo` | `shushizhe` | `zhe`→`zhuo` |
| 0 | `袒护着` | `tanhuzhao` | `tanhuzhe` | `zhe`→`zhao` |
| 0 | `袒护着` | `tanhuzhuo` | `tanhuzhe` | `zhe`→`zhuo` |
| 0 | `趿着` | `tazhao` | `tazhe` | `zhe`→`zhao` |
| 0 | `趿着` | `tazhuo` | `tazhe` | `zhe`→`zhuo` |
| 0 | `提拿着` | `tinazhao` | `tinazhe` | `zhe`→`zhao` |
| 0 | `提拿着` | `tinazhuo` | `tinazhe` | `zhe`→`zhuo` |
| 0 | `剃着` | `tizhao` | `tizhe` | `zhe`→`zhao` |
| 0 | `剃着` | `tizhuo` | `tizhe` | `zhe`→`zhuo` |
| 0 | `玩赏着` | `wanshangzhao` | `wanshangzhe` | `zhe`→`zhao` |
| 0 | `玩赏着` | `wanshangzhuo` | `wanshangzhe` | `zhe`→`zhuo` |
| 0 | `舞弄着` | `wunongzhao` | `wunongzhe` | `zhe`→`zhao` |
| 0 | `舞弄着` | `wunongzhuo` | `wunongzhe` | `zhe`→`zhuo` |
| 0 | `先人着鞭` | `xianrenzhaobian` | `xianrenzhebian` | `zhe`→`zhao` |
| 0 | `先人着鞭` | `xianrenzhuobian` | `xianrenzhebian` | `zhe`→`zhuo` |
| 0 | `显着性` | `xianzhaoxing` | `xianzhexing` | `zhe`→`zhao` |
| 0 | `显着性` | `xianzhuoxing` | `xianzhexing` | `zhe`→`zhuo` |
| 0 | `戏弄着` | `xinongzhao` | `xinongzhe` | `zhe`→`zhao` |
| 0 | `戏弄着` | `xinongzhuo` | `xinongzhe` | `zhe`→`zhuo` |
| 0 | `信誉卓着` | `xinyuzhuozhao` | `xinyuzhuozhe` | `zhe`→`zhao` |
| 0 | `信誉卓着` | `xinyuzhuozhuo` | `xinyuzhuozhe` | `zhe`→`zhuo` |
| 0 | `旋绕着` | `xuanraozhao` | `xuanraozhe` | `zhe`→`zhao` |
| 0 | `旋绕着` | `xuanraozhuo` | `xuanraozhe` | `zhe`→`zhuo` |
| 0 | `蓄积着` | `xujizhao` | `xujizhe` | `zhe`→`zhao` |
| 0 | `蓄积着` | `xujizhuo` | `xujizhe` | `zhe`→`zhuo` |
| 0 | `叙说着` | `xushuozhao` | `xushuozhe` | `zhe`→`zhao` |
| 0 | `叙说着` | `xushuozhuo` | `xushuozhe` | `zhe`→`zhuo` |
| 0 | `仰看着` | `yangkanzhao` | `yangkanzhe` | `zhe`→`zhao` |
| 0 | `仰看着` | `yangkanzhuo` | `yangkanzhe` | `zhe`→`zhuo` |
| 0 | `仰靠着` | `yangkaozhao` | `yangkaozhe` | `zhe`→`zhao` |
| 0 | `仰靠着` | `yangkaozhuo` | `yangkaozhe` | `zhe`→`zhuo` |
| 0 | `仰赖着` | `yanglaizhao` | `yanglaizhe` | `zhe`→`zhao` |
| 0 | `仰赖着` | `yanglaizhuo` | `yanglaizhe` | `zhe`→`zhuo` |
| 0 | `掩护着` | `yanhuzhao` | `yanhuzhe` | `zhe`→`zhao` |
| 0 | `掩护着` | `yanhuzhuo` | `yanhuzhe` | `zhe`→`zhuo` |
| 0 | `咬着唇` | `yaozhaochun` | `yaozhechun` | `zhe`→`zhao` |
| 0 | `咬着唇` | `yaozhuochun` | `yaozhechun` | `zhe`→`zhuo` |
| 0 | `轧着` | `yazhao` | `yazhe` | `zhe`→`zhao` |
| 0 | `轧着` | `yazhuo` | `yazhe` | `zhe`→`zhuo` |
| 0 | `一百一十二着` | `yibaiyishierzhao` | `yibaiyishierzhe` | `zhe`→`zhao` |
| 0 | `一百一十二着` | `yibaiyishierzhuo` | `yibaiyishierzhe` | `zhe`→`zhuo` |
| 0 | `倚靠着` | `yikaozhao` | `yikaozhe` | `zhe`→`zhao` |
| 0 | `倚靠着` | `yikaozhuo` | `yikaozhe` | `zhe`→`zhuo` |
| 0 | `依顺着` | `yishunzhao` | `yishunzhe` | `zhe`→`zhao` |
| 0 | `依顺着` | `yishunzhuo` | `yishunzhe` | `zhe`→`zhuo` |
| 0 | `意谓着` | `yiweizhao` | `yiweizhe` | `zhe`→`zhao` |
| 0 | `意谓着` | `yiweizhuo` | `yiweizhe` | `zhe`→`zhuo` |
| 0 | `依循着` | `yixunzhao` | `yixunzhe` | `zhe`→`zhao` |
| 0 | `依循着` | `yixunzhuo` | `yixunzhe` | `zhe`→`zhuo` |
| 0 | `预藏着` | `yucangzhao` | `yucangzhe` | `zhe`→`zhao` |
| 0 | `预藏着` | `yucangzhuo` | `yucangzhe` | `zhe`→`zhuo` |
| 0 | `仗持着` | `zhangchizhao` | `zhangchizhe` | `zhe`→`zhao` |
| 0 | `仗持着` | `zhangchizhuo` | `zhangchizhe` | `zhe`→`zhuo` |
| 0 | `掌持着` | `zhangchizhao` | `zhangchizhe` | `zhe`→`zhao` |
| 0 | `掌持着` | `zhangchizhuo` | `zhangchizhe` | `zhe`→`zhuo` |
| 0 | `张挂着` | `zhangguazhao` | `zhangguazhe` | `zhe`→`zhao` |
| 0 | `张挂着` | `zhangguazhuo` | `zhangguazhe` | `zhe`→`zhuo` |
| 0 | `展着剂` | `zhanzhaoji` | `zhanzheji` | `zhe`→`zhao` |
| 0 | `展着剂` | `zhanzhuoji` | `zhanzheji` | `zhe`→`zhuo` |
| 0 | `着绩` | `zhaoji` | `zheji` | `zhe`→`zhao` |
| 0 | `着绩` | `zhuoji` | `zheji` | `zhe`→`zhuo` |
| 0 | `着述` | `zhaoshu` | `zheshu` | `zhe`→`zhao` |
| 0 | `着述` | `zhuoshu` | `zheshu` | `zhe`→`zhuo` |
| 0 | `着丝基因` | `zhaosijiyin` | `zhesijiyin` | `zhe`→`zhao` |
| 0 | `着丝基因` | `zhuosijiyin` | `zhesijiyin` | `zhe`→`zhuo` |
| 0 | `着作人` | `zhaozuoren` | `zhezuoren` | `zhe`→`zhao` |
| 0 | `着作人` | `zhuozuoren` | `zhezuoren` | `zhe`→`zhuo` |
| 0 | `奓着胆子` | `zhazhaodanzi` | `zhazhedanzi` | `zhe`→`zhao` |
| 0 | `奓着胆子` | `zhazhuodanzi` | `zhazhedanzi` | `zhe`→`zhuo` |
| 0 | `装模着样` | `zhuangmozhaoyang` | `zhuangmozheyang` | `zhe`→`zhao` |
| 0 | `装模着样` | `zhuangmozhuoyang` | `zhuangmozheyang` | `zhe`→`zhuo` |
| 0 | `拙着` | `zhuozhao` | `zhuozhe` | `zhe`→`zhao` |
| 0 | `拙着` | `zhuozhuo` | `zhuozhe` | `zhe`→`zhuo` |

---

## 字 = `血` (187 rows)

- **Readings**: xuè (书面: 血液/血压/输血/血型) · xiě (口语: 流血/血淋淋/吐血/鸡血/狗血)
- **pypinyin primary**: xuè (书面)
- **Direction stats**:
  - `xue` → `xie` × 187

### Default direction proposal: `del_wrong`

> 书面 xuè 主流;但用户 standing 保留口语 xiě — exception list 列出口语化常用词.

### Exception draft — keep `xie` reading  *(exception: `del_prim`)*

(36 words)

- `吸血` (freq=23311)
- `鸡血` (freq=22830)
- `狗血` (freq=22653)
- `满血` (freq=21578)
- `滴血` (freq=19556)
- `鸭血` (freq=17797)
- `猪血` (freq=17340)
- `血拼` (freq=16741)
- `血槽` (freq=15991)
- `残血` (freq=14487)
- `加血` (freq=13800)
- `掉血` (freq=12866)
- `回血` (not in candidates.tsv)
- `扣血` (not in candidates.tsv)
- `回点血` (not in candidates.tsv)
- `流血` (not in candidates.tsv)
- `吐血` (not in candidates.tsv)
- `抽血` (freq=15282)
- `验血` (not in candidates.tsv)
- `空血` (freq=5930)
- `血崩` (not in candidates.tsv)
- `血赚` (freq=10968)
- `血亏` (not in candidates.tsv)
- `血洗` (not in candidates.tsv)
- `牛血` (not in candidates.tsv)
- `羊血` (freq=10443)
- `蛇血` (not in candidates.tsv)
- `马血` (not in candidates.tsv)
- `泼血` (not in candidates.tsv)
- `见血` (not in candidates.tsv)
- `出血` (not in candidates.tsv)
- `止血` (not in candidates.tsv)
- `尿血` (not in candidates.tsv)
- `血淋淋` (not in candidates.tsv)
- `咳血` (not in candidates.tsv)
- `咯血` (not in candidates.tsv)

### Exception draft — keep `xie_uncertain` reading (UNCERTAIN — keep both)

(7 words)

- `毛血旺` (freq=14919)
- `血馒头` (freq=13509)
- `血豆腐` (not in candidates.tsv)
- `血泡` (not in candidates.tsv)
- `血珠` (not in candidates.tsv)
- `血点` (not in candidates.tsv)
- `血滴` (freq=13087)

### Full word list (all 187 rows, sorted by freq desc)

| freq | word | typed_code | correct_code | dir |
|---|---|---|---|---|
| 23311 | `吸血` | `xixie` | `xixue` | `xue`→`xie` |
| 22830 | `鸡血` | `jixie` | `jixue` | `xue`→`xie` |
| 22653 | `狗血` | `gouxie` | `gouxue` | `xue`→`xie` |
| 21578 | `满血` | `manxie` | `manxue` | `xue`→`xie` |
| 19556 | `滴血` | `dixie` | `dixue` | `xue`→`xie` |
| 18080 | `红血` | `hongxie` | `hongxue` | `xue`→`xie` |
| 17797 | `鸭血` | `yaxie` | `yaxue` | `xue`→`xie` |
| 17340 | `猪血` | `zhuxie` | `zhuxue` | `xue`→`xie` |
| 16893 | `用血` | `yongxie` | `yongxue` | `xue`→`xie` |
| 16741 | `血拼` | `xiepin` | `xuepin` | `xue`→`xie` |
| 16342 | `血雨` | `xieyu` | `xueyu` | `xue`→`xie` |
| 16091 | `捐血` | `juanxie` | `juanxue` | `xue`→`xie` |
| 15991 | `血槽` | `xiecao` | `xuecao` | `xue`→`xie` |
| 15326 | `溢血` | `yixie` | `yixue` | `xue`→`xie` |
| 15282 | `抽血` | `chouxie` | `chouxue` | `xue`→`xie` |
| 15199 | `沥血` | `lixie` | `lixue` | `xue`→`xie` |
| 14993 | `造血` | `zaoxie` | `zaoxue` | `xue`→`xie` |
| 14919 | `毛血旺` | `maoxiewang` | `maoxuewang` | `xue`→`xie` |
| 14487 | `残血` | `canxie` | `canxue` | `xue`→`xie` |
| 14345 | `晕血` | `yunxie` | `yunxue` | `xue`→`xie` |
| 13800 | `加血` | `jiaxie` | `jiaxue` | `xue`→`xie` |
| 13518 | `看血` | `kanxie` | `kanxue` | `xue`→`xie` |
| 13509 | `血馒头` | `xiemantou` | `xuemantou` | `xue`→`xie` |
| 13366 | `血盆` | `xiepen` | `xuepen` | `xue`→`xie` |
| 13113 | `血人` | `xieren` | `xueren` | `xue`→`xie` |
| 13087 | `血滴` | `xiedi` | `xuedi` | `xue`→`xie` |
| 12972 | `血族` | `xiezu` | `xuezu` | `xue`→`xie` |
| 12941 | `血肠` | `xiechang` | `xuechang` | `xue`→`xie` |
| 12866 | `掉血` | `diaoxie` | `diaoxue` | `xue`→`xie` |
| 12505 | `血吐` | `xietu` | `xuetu` | `xue`→`xie` |
| 12396 | `查血` | `chaxie` | `chaxue` | `xue`→`xie` |
| 12089 | `缺血` | `quexie` | `quexue` | `xue`→`xie` |
| 12074 | `饮血` | `yinxie` | `yinxue` | `xue`→`xie` |
| 12066 | `血鸭` | `xieya` | `xueya` | `xue`→`xie` |
| 11826 | `血源` | `xieyuan` | `xueyuan` | `xue`→`xie` |
| 11710 | `喝血` | `hexie` | `hexue` | `xue`→`xie` |
| 11528 | `丝血` | `sixie` | `sixue` | `xue`→`xie` |
| 11528 | `血霉` | `xiemei` | `xuemei` | `xue`→`xie` |
| 11464 | `血橙` | `xiecheng` | `xuecheng` | `xue`→`xie` |
| 11214 | `血凝` | `xiening` | `xuening` | `xue`→`xie` |
| 11206 | `汗血` | `hanxie` | `hanxue` | `xue`→`xie` |
| 11136 | `血症` | `xiezheng` | `xuezheng` | `xue`→`xie` |
| 10968 | `血赚` | `xiezhuan` | `xuezhuan` | `xue`→`xie` |
| 10888 | `血虐` | `xienve` | `xuenve` | `xue`→`xie` |
| 10734 | `血氧` | `xieyang` | `xueyang` | `xue`→`xie` |
| 10721 | `蓝血` | `lanxie` | `lanxue` | `xue`→`xie` |
| 10534 | `血刀` | `xiedao` | `xuedao` | `xue`→`xie` |
| 10443 | `羊血` | `yangxie` | `yangxue` | `xue`→`xie` |
| 9843 | `凝血` | `ningxie` | `ningxue` | `xue`→`xie` |
| 9581 | `动脉血` | `dongmaixie` | `dongmaixue` | `xue`→`xie` |
| 9533 | `血透` | `xietou` | `xuetou` | `xue`→`xie` |
| 9354 | `血站` | `xiezhan` | `xuezhan` | `xue`→`xie` |
| 9327 | `血药` | `xieyao` | `xueyao` | `xue`→`xie` |
| 9313 | `毒血` | `duxie` | `duxue` | `xue`→`xie` |
| 9170 | `血冲` | `xiechong` | `xuechong` | `xue`→`xie` |
| 9137 | `养血` | `yangxie` | `yangxue` | `xue`→`xie` |
| 8913 | `血魔` | `xiemo` | `xuemo` | `xue`→`xie` |
| 8835 | `血检` | `xiejian` | `xuejian` | `xue`→`xie` |
| 8750 | `冒血` | `maoxie` | `maoxue` | `xue`→`xie` |
| 8634 | `坏血` | `huaixie` | `huaixue` | `xue`→`xie` |
| 8518 | `洲血` | `zhouxie` | `zhouxue` | `xue`→`xie` |
| 8104 | `血友` | `xieyou` | `xueyou` | `xue`→`xie` |
| 8097 | `血燕` | `xieyan` | `xueyan` | `xue`→`xie` |
| 8067 | `血情` | `xieqing` | `xueqing` | `xue`→`xie` |
| 8053 | `凝血因子` | `ningxieyinzi` | `ningxueyinzi` | `xue`→`xie` |
| 7789 | `氧血` | `yangxie` | `yangxue` | `xue`→`xie` |
| 7780 | `行血` | `xingxie` | `xingxue` | `xue`→`xie` |
| 7732 | `抗血` | `kangxie` | `kangxue` | `xue`→`xie` |
| 7373 | `血盟` | `xiemeng` | `xuemeng` | `xue`→`xie` |
| 7351 | `满身是血` | `manshenshixie` | `manshenshixue` | `xue`→`xie` |
| 7338 | `抗坏血酸` | `kanghuaixiesuan` | `kanghuaixuesuan` | `xue`→`xie` |
| 7233 | `血手印` | `xieshouyin` | `xueshouyin` | `xue`→`xie` |
| 7233 | `有血有泪` | `youxieyoulei` | `youxueyoulei` | `xue`→`xie` |
| 7167 | `血与火` | `xieyuhuo` | `xueyuhuo` | `xue`→`xie` |
| 6801 | `捐血人` | `juanxieren` | `juanxueren` | `xue`→`xie` |
| 6655 | `龙血树` | `longxieshu` | `longxueshu` | `xue`→`xie` |
| 6638 | `静脉血` | `jingmaixie` | `jingmaixue` | `xue`→`xie` |
| 6638 | `血脑屏障` | `xienaopingzhang` | `xuenaopingzhang` | `xue`→`xie` |
| 6559 | `血常规` | `xiechanggui` | `xuechanggui` | `xue`→`xie` |
| 6528 | `以血还血` | `yixiehaixie` | `yixuehaixue` | `xue`→`xie` |
| 6528 | `以血还血` | `yixiehaixue` | `yixuehaixue` | `xue`→`xie` |
| 6528 | `以血还血` | `yixiehuanxie` | `yixuehaixue` | `xue`→`xie` |
| 6528 | `以血还血` | `yixiehuanxue` | `yixuehaixue` | `xue`→`xie` |
| 6528 | `以血还血` | `yixuehaixie` | `yixuehaixue` | `xue`→`xie` |
| 6455 | `血药浓度` | `xieyaonongdu` | `xueyaonongdu` | `xue`→`xie` |
| 6442 | `吸血虫` | `xixiechong` | `xixuechong` | `xue`→`xie` |
| 6265 | `隐血` | `yinxie` | `yinxue` | `xue`→`xie` |
| 6209 | `抗凝血` | `kangningxie` | `kangningxue` | `xue`→`xie` |
| 6117 | `血光` | `xieguang` | `xueguang` | `xue`→`xie` |
| 6105 | `血凝素` | `xieningsu` | `xueningsu` | `xue`→`xie` |
| 6064 | `血刀老祖` | `xiedaolaozu` | `xuedaolaozu` | `xue`→`xie` |
| 5974 | `坏血酸` | `huaixiesuan` | `huaixuesuan` | `xue`→`xie` |
| 5930 | `费血` | `feixie` | `feixue` | `xue`→`xie` |
| 5930 | `空血` | `kongxie` | `kongxue` | `xue`→`xie` |
| 5930 | `蓝血人` | `lanxieren` | `lanxueren` | `xue`→`xie` |
| 5930 | `血耳` | `xieer` | `xueer` | `xue`→`xie` |
| 5577 | `血源性` | `xieyuanxing` | `xueyuanxing` | `xue`→`xie` |
| 5526 | `温血动物` | `wenxiedongwu` | `wenxuedongwu` | `xue`→`xie` |
| 5354 | `受血者` | `shouxiezhe` | `shouxuezhe` | `xue`→`xie` |
| 5354 | `外周血` | `waizhouxie` | `waizhouxue` | `xue`→`xie` |
| 5286 | `赤血` | `chixie` | `chixue` | `xue`→`xie` |
| 5167 | `血荐轩辕` | `xiejianxuanyuan` | `xuejianxuanyuan` | `xue`→`xie` |
| 4747 | `耗血` | `haoxie` | `haoxue` | `xue`→`xie` |
| 4747 | `血粘度` | `xiezhandu` | `xuezhandu` | `xue`→`xie` |
| 4495 | `供血者` | `gongxiezhe` | `gongxuezhe` | `xue`→`xie` |
| 4495 | `血河` | `xiehe` | `xuehe` | `xue`→`xie` |
| 4422 | `血玲珑` | `xielinglong` | `xuelinglong` | `xue`→`xie` |
| 4422 | `血鹦鹉` | `xieyingwu` | `xueyingwu` | `xue`→`xie` |
| 4422 | `血债血还` | `xuezhaixiehai` | `xuezhaixuehai` | `xue`→`xie` |
| 4422 | `血债血还` | `xuezhaixiehuan` | `xuezhaixuehai` | `xue`→`xie` |
| 4422 | `张开血` | `zhangkaixie` | `zhangkaixue` | `xue`→`xie` |
| 4131 | `血汁` | `xiezhi` | `xuezhi` | `xue`→`xie` |
| 4131 | `血制品` | `xiezhipin` | `xuezhipin` | `xue`→`xie` |
| 3861 | `脑缺血` | `naoquexie` | `naoquexue` | `xue`→`xie` |
| 3686 | `积血` | `jixie` | `jixue` | `xue`→`xie` |
| 3468 | `血素` | `xiesu` | `xuesu` | `xue`→`xie` |
| 3094 | `捐血车` | `juanxieche` | `juanxueche` | `xue`→`xie` |
| 3094 | `损血` | `sunxie` | `sunxue` | `xue`→`xie` |
| 3094 | `污血` | `wuxie` | `wuxue` | `xue`→`xie` |
| 3094 | `血脖` | `xiebo` | `xuebo` | `xue`→`xie` |
| 3094 | `血荒` | `xiehuang` | `xuehuang` | `xue`→`xie` |
| 3094 | `血饮` | `xieyin` | `xueyin` | `xue`→`xie` |
| 2987 | `抗凝血酶` | `kangningxiemei` | `kangningxuemei` | `xue`→`xie` |
| 2987 | `利血平` | `lixieping` | `lixueping` | `xue`→`xie` |
| 2987 | `血瘀` | `xieyu` | `xueyu` | `xue`→`xie` |
| 2367 | `大血藤` | `daxieteng` | `daxueteng` | `xue`→`xie` |
| 2367 | `低钾血症` | `dijiaxiezheng` | `dijiaxuezheng` | `xue`→`xie` |
| 2367 | `毒血症` | `duxiezheng` | `duxuezheng` | `xue`→`xie` |
| 2367 | `高钠血症` | `gaonaxiezheng` | `gaonaxuezheng` | `xue`→`xie` |
| 2367 | `高脂血症` | `gaozhixiezheng` | `gaozhixuezheng` | `xue`→`xie` |
| 2367 | `钾血症` | `jiaxiezheng` | `jiaxuezheng` | `xue`→`xie` |
| 2367 | `菌血症` | `junxiezheng` | `junxuezheng` | `xue`→`xie` |
| 2367 | `匈奴血` | `xiongnuxie` | `xiongnuxue` | `xue`→`xie` |
| 1493 | `病毒血症` | `bingduxiezheng` | `bingduxuezheng` | `xue`→`xie` |
| 1493 | `初血` | `chuxie` | `chuxue` | `xue`→`xie` |
| 1493 | `凝血药` | `ningxieyao` | `ningxueyao` | `xue`→`xie` |
| 1493 | `心肌缺血` | `xinjiquexie` | `xinjiquexue` | `xue`→`xie` |
| 0 | `安络血` | `anluoxie` | `anluoxue` | `xue`→`xie` |
| 0 | `报仇血耻` | `baochouxiechi` | `baochouxuechi` | `xue`→`xie` |
| 0 | `采供血` | `caigongxie` | `caigongxue` | `xue`→`xie` |
| 0 | `赤血魔剑` | `chixiemojian` | `chixuemojian` | `xue`→`xie` |
| 0 | `纯血马` | `chunxiema` | `chunxuema` | `xue`→`xie` |
| 0 | `刀不刃血` | `daoburenxie` | `daoburenxue` | `xue`→`xie` |
| 0 | `低钠血症` | `dinaxiezheng` | `dinaxuezheng` | `xue`→`xie` |
| 0 | `滴血钻石` | `dixiezuanshi` | `dixuezuanshi` | `xue`→`xie` |
| 0 | `凤凰血` | `fenghuangxie` | `fenghuangxue` | `xue`→`xie` |
| 0 | `高钙血症` | `gaogaixiezheng` | `gaogaixuezheng` | `xue`→`xie` |
| 0 | `高钾血症` | `gaojiaxiezheng` | `gaojiaxuezheng` | `xue`→`xie` |
| 0 | `老溢血` | `laoyixie` | `laoyixue` | `xue`→`xie` |
| 0 | `凉血动物` | `liangxiedongwu` | `liangxuedongwu` | `xue`→`xie` |
| 0 | `凝血剂` | `ningxieji` | `ningxueji` | `xue`→`xie` |
| 0 | `凝血烷` | `ningxiewan` | `ningxuewan` | `xue`→`xie` |
| 0 | `浓血症` | `nongxiezheng` | `nongxuezheng` | `xue`→`xie` |
| 0 | `沤心沥血` | `ouxinlixie` | `ouxinlixue` | `xue`→`xie` |
| 0 | `喷血` | `penxie` | `penxue` | `xue`→`xie` |
| 0 | `脐带血` | `qidaixie` | `qidaixue` | `xue`→`xie` |
| 0 | `荣禄血` | `rongluxie` | `rongluxue` | `xue`→`xie` |
| 0 | `时血勇` | `shixieyong` | `shixueyong` | `xue`→`xie` |
| 0 | `嗽血` | `souxie` | `souxue` | `xue`→`xie` |
| 0 | `血卟啉` | `xiebulin` | `xuebulin` | `xue`→`xie` |
| 0 | `血府逐瘀汤` | `xiefuzhuyutang` | `xuefuzhuyutang` | `xue`→`xie` |
| 0 | `血钙质` | `xiegaizhi` | `xuegaizhi` | `xue`→`xie` |
| 0 | `血行器` | `xiehangqi` | `xuexingqi` | `xue`→`xie` |
| 0 | `血行器` | `xiexingqi` | `xuexingqi` | `xue`→`xie` |
| 0 | `血科` | `xieke` | `xueke` | `xue`→`xie` |
| 0 | `血骷髅` | `xiekulou` | `xuekulou` | `xue`→`xie` |
| 0 | `血轮` | `xielun` | `xuelun` | `xue`→`xie` |
| 0 | `血青素` | `xieqingsu` | `xueqingsu` | `xue`→`xie` |
| 0 | `血热型` | `xierexing` | `xuerexing` | `xue`→`xie` |
| 0 | `血塞通` | `xiesaitong` | `xuesaitong` | `xue`→`xie` |
| 0 | `血塞通` | `xiesetong` | `xuesaitong` | `xue`→`xie` |
| 0 | `血汙` | `xiewu` | `xuewu` | `xue`→`xie` |
| 0 | `血线虫` | `xiexianchong` | `xuexianchong` | `xue`→`xie` |
| 0 | `血绣` | `xiexiu` | `xuexiu` | `xue`→`xie` |
| 0 | `血影蛋白` | `xieyingdanbai` | `xueyingdanbai` | `xue`→`xie` |
| 0 | `血涌如注` | `xieyongruzhu` | `xueyongruzhu` | `xue`→`xie` |
| 0 | `血域` | `xieyu` | `xueyu` | `xue`→`xie` |
| 0 | `血域龙吟` | `xieyulongyin` | `xueyulongyin` | `xue`→`xie` |
| 0 | `血运重建` | `xieyunchongjian` | `xueyunchongjian` | `xue`→`xie` |
| 0 | `血余炭` | `xieyutan` | `xueyutan` | `xue`→`xie` |
| 0 | `血诏` | `xiezhao` | `xuezhao` | `xue`→`xie` |
| 0 | `异抗坏血酸钠` | `yikanghuaixiesuanna` | `yikanghuaixuesuanna` | `xue`→`xie` |
| 0 | `益气养血` | `yiqiyangxie` | `yiqiyangxue` | `xue`→`xie` |
| 0 | `以血偿血` | `yixiechangxie` | `yixuechangxue` | `xue`→`xie` |
| 0 | `以血偿血` | `yixiechangxue` | `yixuechangxue` | `xue`→`xie` |
| 0 | `以血偿血` | `yixuechangxie` | `yixuechangxue` | `xue`→`xie` |
| 0 | `郁血` | `yuxie` | `yuxue` | `xue`→`xie` |

---

## 字 = `调` (229 rows)

- **Readings**: tiáo (动词: 调节/调整/调味/调高/调音/微调) · diào (动词: 调动/调任/调走/调度/调查 · 名词: 音调/曲调/语调)
- **pypinyin primary**: diào (但很多 tiáo 词被误 primary 成 diào)
- **Direction stats**:
  - `diao` → `tiao` × 229

### Default direction proposal: `del_prim`

> ★ direction 反 ★ pypinyin 把动词 tiáo 类(调高/调低/调好/调整)误标为 primary diào;实际 tiáo 才是这些词的主流读音 → default 删 pypinyin 的 primary copy(=del_prim),保留 tiao 读音.

### Exception draft — keep `diao` reading  *(exception: `del_wrong`)*

(50 words)

- `调动` (not in candidates.tsv)
- `调走` (freq=14030)
- `调任` (not in candidates.tsv)
- `调出` (freq=23102)
- `调入` (freq=14073)
- `调离` (not in candidates.tsv)
- `调到` (freq=21563)
- `调来` (freq=16635)
- `调去` (freq=15628)
- `调度` (not in candidates.tsv)
- `调遣` (not in candidates.tsv)
- `调兵` (freq=14612)
- `调研` (not in candidates.tsv)
- `调用` (not in candidates.tsv)
- `调查` (not in candidates.tsv)
- `调阅` (not in candidates.tsv)
- `音调` (not in candidates.tsv)
- `曲调` (not in candidates.tsv)
- `语调` (not in candidates.tsv)
- `声调` (not in candidates.tsv)
- `腔调` (not in candidates.tsv)
- `格调` (not in candidates.tsv)
- `步调` (not in candidates.tsv)
- `笔调` (not in candidates.tsv)
- `低调` (not in candidates.tsv)
- `高调` (not in candidates.tsv)
- `色调` (not in candidates.tsv)
- `主调` (not in candidates.tsv)
- `基调` (not in candidates.tsv)
- `定调` (not in candidates.tsv)
- `唱调` (not in candidates.tsv)
- `跑调` (not in candidates.tsv)
- `走调` (not in candidates.tsv)
- `变调` (not in candidates.tsv)
- `转调` (not in candidates.tsv)
- `调子` (not in candidates.tsv)
- `调头` (not in candidates.tsv)
- `调头转向` (not in candidates.tsv)
- `老调` (not in candidates.tsv)
- `新调` (not in candidates.tsv)
- `唱反调` (not in candidates.tsv)
- `调虎离山` (not in candidates.tsv)
- `调包` (not in candidates.tsv)
- `派调` (freq=7187)
- `委派调` (not in candidates.tsv)
- `回调` (freq=4277)
- `上调` (not in candidates.tsv)
- `下调` (not in candidates.tsv)
- `总调` (not in candidates.tsv)
- `总调度` (not in candidates.tsv)

### Full word list (all 229 rows, sorted by freq desc)

| freq | word | typed_code | correct_code | dir |
|---|---|---|---|---|
| 23310 | `调低` | `tiaodi` | `diaodi` | `diao`→`tiao` |
| 23102 | `调出` | `tiaochu` | `diaochu` | `diao`→`tiao` |
| 21563 | `调到` | `tiaodao` | `diaodao` | `diao`→`tiao` |
| 21225 | `调好` | `tiaohao` | `diaohao` | `diao`→`tiao` |
| 19318 | `不调` | `butiao` | `budiao` | `diao`→`tiao` |
| 18921 | `调休` | `tiaoxiu` | `diaoxiu` | `diao`→`tiao` |
| 18836 | `调开` | `tiaokai` | `diaokai` | `diao`→`tiao` |
| 18435 | `可调` | `ketiao` | `kediao` | `diao`→`tiao` |
| 17992 | `调过` | `tiaoguo` | `diaoguo` | `diao`→`tiao` |
| 17575 | `调成` | `tiaocheng` | `diaocheng` | `diao`→`tiao` |
| 16913 | `微调` | `weitiao` | `weidiao` | `diao`→`tiao` |
| 16776 | `调下` | `tiaoxia` | `diaoxia` | `diao`→`tiao` |
| 16635 | `调来` | `tiaolai` | `diaolai` | `diao`→`tiao` |
| 16030 | `调车` | `tiaoche` | `diaoche` | `diao`→`tiao` |
| 15875 | `调酒` | `tiaojiu` | `diaojiu` | `diao`→`tiao` |
| 15639 | `调高` | `tiaogao` | `diaogao` | `diao`→`tiao` |
| 15628 | `调去` | `tiaoqu` | `diaoqu` | `diao`→`tiao` |
| 15531 | `调上` | `tiaoshang` | `diaoshang` | `diao`→`tiao` |
| 14927 | `内调` | `neitiao` | `neidiao` | `diao`→`tiao` |
| 14736 | `调得` | `tiaode` | `diaode` | `diao`→`tiao` |
| 14736 | `调得` | `tiaodei` | `diaode` | `diao`→`tiao` |
| 14612 | `调兵` | `tiaobing` | `diaobing` | `diao`→`tiao` |
| 14545 | `调过来` | `tiaoguolai` | `diaoguolai` | `diao`→`tiao` |
| 14289 | `调给` | `tiaogei` | `diaogei` | `diao`→`tiao` |
| 14289 | `调给` | `tiaoji` | `diaogei` | `diao`→`tiao` |
| 14073 | `调入` | `tiaoru` | `diaoru` | `diao`→`tiao` |
| 14040 | `民调` | `mintiao` | `mindiao` | `diao`→`tiao` |
| 14030 | `调走` | `tiaozou` | `diaozou` | `diao`→`tiao` |
| 13923 | `调班` | `tiaoban` | `diaoban` | `diao`→`tiao` |
| 13867 | `加调` | `jiatiao` | `jiadiao` | `diao`→`tiao` |
| 13779 | `先调` | `xiantiao` | `xiandiao` | `diao`→`tiao` |
| 13684 | `出调` | `chutiao` | `chudiao` | `diao`→`tiao` |
| 13488 | `还调` | `haitiao` | `haidiao` | `diao`→`tiao` |
| 13092 | `调起` | `tiaoqi` | `diaoqi` | `diao`→`tiao` |
| 12945 | `调为` | `tiaowei` | `diaowei` | `diao`→`tiao` |
| 12710 | `调酒师` | `tiaojiushi` | `diaojiushi` | `diao`→`tiao` |
| 12671 | `越调` | `yuetiao` | `yuediao` | `diao`→`tiao` |
| 12532 | `南水北调` | `nanshuibeitiao` | `nanshuibeidiao` | `diao`→`tiao` |
| 12507 | `调监` | `tiaojian` | `diaojian` | `diao`→`tiao` |
| 12473 | `合调` | `hetiao` | `hediao` | `diao`→`tiao` |
| 12330 | `调快` | `tiaokuai` | `diaokuai` | `diao`→`tiao` |
| 12237 | `搭调` | `datiao` | `dadiao` | `diao`→`tiao` |
| 11997 | `调水` | `tiaoshui` | `diaoshui` | `diao`→`tiao` |
| 11731 | `调至` | `tiaozhi` | `diaozhi` | `diao`→`tiao` |
| 11704 | `调性` | `tiaoxing` | `diaoxing` | `diao`→`tiao` |
| 11651 | `调货` | `tiaohuo` | `diaohuo` | `diao`→`tiao` |
| 11647 | `调重` | `tiaochong` | `diaozhong` | `diao`→`tiao` |
| 11647 | `调重` | `tiaozhong` | `diaozhong` | `diao`→`tiao` |
| 11444 | `调岗` | `tiaogang` | `diaogang` | `diao`→`tiao` |
| 11193 | `乱调` | `luantiao` | `luandiao` | `diao`→`tiao` |
| 11156 | `升调` | `shengtiao` | `shengdiao` | `diao`→`tiao` |
| 11153 | `调压` | `tiaoya` | `diaoya` | `diao`→`tiao` |
| 11113 | `带调` | `daitiao` | `daidiao` | `diao`→`tiao` |
| 11108 | `复调` | `futiao` | `fudiao` | `diao`→`tiao` |
| 10888 | `瞎调` | `xiatiao` | `xiadiao` | `diao`→`tiao` |
| 10853 | `调往` | `tiaowang` | `diaowang` | `diao`→`tiao` |
| 10844 | `气调` | `qitiao` | `qidiao` | `diao`→`tiao` |
| 10819 | `调白` | `tiaobai` | `diaobai` | `diao`→`tiao` |
| 10713 | `自调` | `zitiao` | `zidiao` | `diao`→`tiao` |
| 10634 | `调过去` | `tiaoguoqu` | `diaoguoqu` | `diao`→`tiao` |
| 10547 | `调相` | `tiaoxiang` | `diaoxiang` | `diao`→`tiao` |
| 10457 | `歌调` | `getiao` | `gediao` | `diao`→`tiao` |
| 10448 | `橘调` | `jutiao` | `judiao` | `diao`→`tiao` |
| 10448 | `预调` | `yutiao` | `yudiao` | `diao`→`tiao` |
| 10333 | `句调` | `jutiao` | `judiao` | `diao`→`tiao` |
| 10249 | `调兵山` | `tiaobingshan` | `diaobingshan` | `diao`→`tiao` |
| 9952 | `调速` | `tiaosu` | `diaosu` | `diao`→`tiao` |
| 9862 | `调校` | `tiaoxiao` | `diaoxiao` | `diao`→`tiao` |
| 9584 | `调类` | `tiaolei` | `diaolei` | `diao`→`tiao` |
| 9533 | `调馅` | `tiaoxian` | `diaoxian` | `diao`→`tiao` |
| 9533 | `少调` | `shaotiao` | `shaodiao` | `diao`→`tiao` |
| 9503 | `调弦` | `tiaoxian` | `diaoxian` | `diao`→`tiao` |
| 9458 | `乐调` | `letiao` | `lediao` | `diao`→`tiao` |
| 9084 | `调位` | `tiaowei` | `diaowei` | `diao`→`tiao` |
| 9012 | `调差` | `tiaocha` | `diaocha` | `diao`→`tiao` |
| 9012 | `调差` | `tiaochai` | `diaocha` | `diao`→`tiao` |
| 8988 | `调进` | `tiaojin` | `diaojin` | `diao`→`tiao` |
| 8731 | `冷调` | `lengtiao` | `lengdiao` | `diao`→`tiao` |
| 8595 | `互调` | `hutiao` | `hudiao` | `diao`→`tiao` |
| 8552 | `京调` | `jingtiao` | `jingdiao` | `diao`→`tiao` |
| 8517 | `无调` | `wutiao` | `wudiao` | `diao`→`tiao` |
| 8470 | `标调` | `biaotiao` | `biaodiao` | `diao`→`tiao` |
| 8323 | `调香` | `tiaoxiang` | `diaoxiang` | `diao`→`tiao` |
| 8091 | `调来调去` | `diaolaitiaoqu` | `diaolaidiaoqu` | `diao`→`tiao` |
| 8091 | `调来调去` | `tiaolaidiaoqu` | `diaolaidiaoqu` | `diao`→`tiao` |
| 8091 | `调来调去` | `tiaolaitiaoqu` | `diaolaidiaoqu` | `diao`→`tiao` |
| 7836 | `彩调` | `caitiao` | `caidiao` | `diao`→`tiao` |
| 7824 | `调减` | `tiaojian` | `diaojian` | `diao`→`tiao` |
| 7791 | `龙船调` | `longchuantiao` | `longchuandiao` | `diao`→`tiao` |
| 7715 | `调使` | `tiaoshi` | `diaoshi` | `diao`→`tiao` |
| 7695 | `调蓄` | `tiaoxu` | `diaoxu` | `diao`→`tiao` |
| 7660 | `律调` | `lvtiao` | `lvdiao` | `diao`→`tiao` |
| 7233 | `偷调` | `toutiao` | `toudiao` | `diao`→`tiao` |
| 7187 | `调近` | `tiaojin` | `diaojin` | `diao`→`tiao` |
| 7187 | `派调` | `paitiao` | `paidiao` | `diao`→`tiao` |
| 7130 | `调琴` | `tiaoqin` | `diaoqin` | `diao`→`tiao` |
| 7130 | `换调` | `huantiao` | `huandiao` | `diao`→`tiao` |
| 7099 | `调角` | `tiaojiao` | `diaojiao` | `diao`→`tiao` |
| 7099 | `调角` | `tiaojue` | `diaojiao` | `diao`→`tiao` |
| 6927 | `函调` | `hantiao` | `handiao` | `diao`→`tiao` |
| 6868 | `调出去` | `tiaochuqu` | `diaochuqu` | `diao`→`tiao` |
| 6868 | `琴调` | `qintiao` | `qindiao` | `diao`→`tiao` |
| 6820 | `调训` | `tiaoxun` | `diaoxun` | `diao`→`tiao` |
| 6818 | `调峰` | `tiaofeng` | `diaofeng` | `diao`→`tiao` |
| 6487 | `调流` | `tiaoliu` | `diaoliu` | `diao`→`tiao` |
| 6463 | `调轻` | `tiaoqing` | `diaoqing` | `diao`→`tiao` |
| 6442 | `调下去` | `tiaoxiaqu` | `diaoxiaqu` | `diao`→`tiao` |
| 6442 | `调薪` | `tiaoxin` | `diaoxin` | `diao`→`tiao` |
| 6380 | `普调` | `putiao` | `pudiao` | `diao`→`tiao` |
| 6117 | `调寄` | `tiaoji` | `diaoji` | `diao`→`tiao` |
| 5930 | `调强` | `tiaoqiang` | `diaoqiang` | `diao`→`tiao` |
| 5669 | `调露` | `tiaolu` | `diaolu` | `diao`→`tiao` |
| 5633 | `调名` | `tiaoming` | `diaoming` | `diao`→`tiao` |
| 5286 | `调钱` | `tiaoqian` | `diaoqian` | `diao`→`tiao` |
| 5286 | `苦调` | `kutiao` | `kudiao` | `diao`→`tiao` |
| 5167 | `调上来` | `tiaoshanglai` | `diaoshanglai` | `diao`→`tiao` |
| 5167 | `首调` | `shoutiao` | `shoudiao` | `diao`→`tiao` |
| 5167 | `宣叙调` | `xuanxutiao` | `xuanxudiao` | `diao`→`tiao` |
| 5073 | `移调` | `yitiao` | `yidiao` | `diao`→`tiao` |
| 4842 | `调远` | `tiaoyuan` | `diaoyuan` | `diao`→`tiao` |
| 4804 | `黄梅调` | `huangmeitiao` | `huangmeidiao` | `diao`→`tiao` |
| 4734 | `祥符调` | `xiangfutiao` | `xiangfudiao` | `diao`→`tiao` |
| 4652 | `调增` | `tiaozeng` | `diaozeng` | `diao`→`tiao` |
| 4643 | `谐调` | `xietiao` | `xiediao` | `diao`→`tiao` |
| 4495 | `调细` | `tiaoxi` | `diaoxi` | `diao`→`tiao` |
| 4495 | `调向` | `tiaoxiang` | `diaoxiang` | `diao`→`tiao` |
| 4495 | `联调` | `liantiao` | `liandiao` | `diao`→`tiao` |
| 4480 | `月经不调` | `yuejingbutiao` | `yuejingbudiao` | `diao`→`tiao` |
| 4422 | `北水南调` | `beishuinantiao` | `beishuinandiao` | `diao`→`tiao` |
| 4422 | `调进去` | `tiaojinqu` | `diaojinqu` | `diao`→`tiao` |
| 4422 | `调嗓子` | `tiaosangzi` | `diaosangzi` | `diao`→`tiao` |
| 4422 | `调上去` | `tiaoshangqu` | `diaoshangqu` | `diao`→`tiao` |
| 4422 | `花香调` | `huaxiangtiao` | `huaxiangdiao` | `diao`→`tiao` |
| 4422 | `匀调` | `yuntiao` | `yundiao` | `diao`→`tiao` |
| 4277 | `回调` | `huitiao` | `huidiao` | `diao`→`tiao` |
| 4193 | `调车场` | `tiaochechang` | `diaochechang` | `diao`→`tiao` |
| 3861 | `调移` | `tiaoyi` | `diaoyi` | `diao`→`tiao` |
| 3468 | `列调` | `lietiao` | `liediao` | `diao`→`tiao` |
| 3094 | `调测` | `tiaoce` | `diaoce` | `diao`→`tiao` |
| 3094 | `调妥` | `tiaotuo` | `diaotuo` | `diao`→`tiao` |
| 3094 | `调位子` | `tiaoweizi` | `diaoweizi` | `diao`→`tiao` |
| 3094 | `调销` | `tiaoxiao` | `diaoxiao` | `diao`→`tiao` |
| 3094 | `哭调` | `kutiao` | `kudiao` | `diao`→`tiao` |
| 3094 | `迁调` | `qiantiao` | `qiandiao` | `diao`→`tiao` |
| 3094 | `现调机` | `xiantiaoji` | `xiandiaoji` | `diao`→`tiao` |
| 3094 | `暂调` | `zantiao` | `zandiao` | `diao`→`tiao` |
| 2987 | `调赴` | `tiaofu` | `diaofu` | `diao`→`tiao` |
| 2987 | `调迁` | `tiaoqian` | `diaoqian` | `diao`→`tiao` |
| 2987 | `调升` | `tiaosheng` | `diaosheng` | `diao`→`tiao` |
| 2987 | `调五事` | `tiaowushi` | `diaowushi` | `diao`→`tiao` |
| 2987 | `调御` | `tiaoyu` | `diaoyu` | `diao`→`tiao` |
| 2987 | `轮调` | `luntiao` | `lundiao` | `diao`→`tiao` |
| 2987 | `免调` | `miantiao` | `miandiao` | `diao`→`tiao` |
| 2987 | `一平二调` | `yipingertiao` | `yipingerdiao` | `diao`→`tiao` |
| 2367 | `调济` | `tiaoji` | `diaoji` | `diao`→`tiao` |
| 2367 | `调宽` | `tiaokuan` | `diaokuan` | `diao`→`tiao` |
| 2367 | `调压井` | `tiaoyajing` | `diaoyajing` | `diao`→`tiao` |
| 2367 | `官调` | `guantiao` | `guandiao` | `diao`→`tiao` |
| 2367 | `军调部` | `juntiaobu` | `jundiaobu` | `diao`→`tiao` |
| 2367 | `拟调` | `nitiao` | `nidiao` | `diao`→`tiao` |
| 1493 | `调拌` | `tiaoban` | `diaoban` | `diao`→`tiao` |
| 1493 | `调进来` | `tiaojinlai` | `diaojinlai` | `diao`→`tiao` |
| 1493 | `吴调侯` | `wutiaohou` | `wudiaohou` | `diao`→`tiao` |
| 1493 | `紫竹调` | `zizhutiao` | `zizhudiao` | `diao`→`tiao` |
| 0 | `癌调蛋白` | `aitiaodanbai` | `aidiaodanbai` | `diao`→`tiao` |
| 0 | `边调鹤` | `biantiaohe` | `biandiaohe` | `diao`→`tiao` |
| 0 | `波束调向` | `boshutiaoxiang` | `boshudiaoxiang` | `diao`→`tiao` |
| 0 | `不合调` | `buhetiao` | `buhediao` | `diao`→`tiao` |
| 0 | `城调队` | `chengtiaodui` | `chengdiaodui` | `diao`→`tiao` |
| 0 | `粗调` | `cutiao` | `cudiao` | `diao`→`tiao` |
| 0 | `粗腔横调` | `cuqianghengtiao` | `cuqianghengdiao` | `diao`→`tiao` |
| 0 | `带有色调` | `daiyousetiao` | `daiyousediao` | `diao`→`tiao` |
| 0 | `单色谐调` | `dansexietiao` | `dansexiediao` | `diao`→`tiao` |
| 0 | `电压可调` | `dianyaketiao` | `dianyakediao` | `diao`→`tiao` |
| 0 | `调兵山市` | `tiaobingshanshi` | `diaobingshanshi` | `diao`→`tiao` |
| 0 | `调关镇` | `tiaoguanzhen` | `diaoguanzhen` | `diao`→`tiao` |
| 0 | `调画出` | `tiaohuachu` | `diaohuachu` | `diao`→`tiao` |
| 0 | `调聚反应` | `tiaojufanying` | `diaojufanying` | `diao`→`tiao` |
| 0 | `调慑` | `tiaoshe` | `diaoshe` | `diao`→`tiao` |
| 0 | `调湿` | `tiaoshi` | `diaoshi` | `diao`→`tiao` |
| 0 | `调速器` | `tiaosuqi` | `diaosuqi` | `diao`→`tiao` |
| 0 | `调委会` | `tiaoweihui` | `diaoweihui` | `diao`→`tiao` |
| 0 | `调委会` | `tiaoweikuai` | `diaoweihui` | `diao`→`tiao` |
| 0 | `调下来` | `tiaoxialai` | `diaoxialai` | `diao`→`tiao` |
| 0 | `调向叠加` | `tiaoxiangdiejia` | `diaoxiangdiejia` | `diao`→`tiao` |
| 0 | `调相器` | `tiaoxiangqi` | `diaoxiangqi` | `diao`→`tiao` |
| 0 | `调校器` | `tiaoxiaoqi` | `diaoxiaoqi` | `diao`→`tiao` |
| 0 | `调压阀` | `tiaoyafa` | `diaoyafa` | `diao`→`tiao` |
| 0 | `调压器` | `tiaoyaqi` | `diaoyaqi` | `diao`→`tiao` |
| 0 | `调压室` | `tiaoyashi` | `diaoyashi` | `diao`→`tiao` |
| 0 | `调冶` | `tiaoye` | `diaoye` | `diao`→`tiao` |
| 0 | `调账` | `tiaozhang` | `diaozhang` | `diao`→`tiao` |
| 0 | `调置` | `tiaozhi` | `diaozhi` | `diao`→`tiao` |
| 0 | `调质` | `tiaozhi` | `diaozhi` | `diao`→`tiao` |
| 0 | `杜里调` | `dulitiao` | `dulidiao` | `diao`→`tiao` |
| 0 | `罚调` | `fatiao` | `fadiao` | `diao`→`tiao` |
| 0 | `非移调` | `feiyitiao` | `feiyidiao` | `diao`→`tiao` |
| 0 | `风顺雨调` | `fengshunyutiao` | `fengshunyudiao` | `diao`→`tiao` |
| 0 | `钙调素` | `gaitiaosu` | `gaidiaosu` | `diao`→`tiao` |
| 0 | `官腔官调` | `guanqiangguantiao` | `guanqiangguandiao` | `diao`→`tiao` |
| 0 | `古调独弹` | `gutiaodudan` | `gudiaodudan` | `diao`→`tiao` |
| 0 | `古调独弹` | `gutiaodutan` | `gudiaodudan` | `diao`→`tiao` |
| 0 | `蒋调侯` | `jiangtiaohou` | `jiangdiaohou` | `diao`→`tiao` |
| 0 | `机电调向` | `jidiantiaoxiang` | `jidiandiaoxiang` | `diao`→`tiao` |
| 0 | `肌调蛋白` | `jitiaodanbai` | `jidiaodanbai` | `diao`→`tiao` |
| 0 | `冷色调` | `lengsetiao` | `lengsediao` | `diao`→`tiao` |
| 0 | `李调元` | `litiaoyuan` | `lidiaoyuan` | `diao`→`tiao` |
| 0 | `轮调法` | `luntiaofa` | `lundiaofa` | `diao`→`tiao` |
| 0 | `吕调阳` | `lvtiaoyang` | `lvdiaoyang` | `diao`→`tiao` |
| 0 | `吕季调` | `lvjitiao` | `lvjidiao` | `diao`→`tiao` |
| 0 | `满桂调` | `manguitiao` | `manguidiao` | `diao`→`tiao` |
| 0 | `南粮北调` | `nanliangbeitiao` | `nanliangbeidiao` | `diao`→`tiao` |
| 0 | `捏腔拿调` | `nieqiangnatiao` | `nieqiangnadiao` | `diao`→`tiao` |
| 0 | `平面调车` | `pingmiantiaoche` | `pingmiandiaoche` | `diao`→`tiao` |
| 0 | `诗调` | `shitiao` | `shidiao` | `diao`→`tiao` |
| 0 | `竖调` | `shutiao` | `shudiao` | `diao`→`tiao` |
| 0 | `速调管` | `sutiaoguan` | `sudiaoguan` | `diao`→`tiao` |
| 0 | `王尔调` | `wangertiao` | `wangerdiao` | `diao`→`tiao` |
| 0 | `旋调管` | `xuantiaoguan` | `xuandiaoguan` | `diao`→`tiao` |
| 0 | `徐调孚` | `xutiaofu` | `xudiaofu` | `diao`→`tiao` |
| 0 | `哑调` | `yatiao` | `yadiao` | `diao`→`tiao` |
| 0 | `杨诚调` | `yangchengtiao` | `yangchengdiao` | `diao`→`tiao` |
| 0 | `一调之下` | `yitiaozhixia` | `yidiaozhixia` | `diao`→`tiao` |
| 0 | `元廷调` | `yuantingtiao` | `yuantingdiao` | `diao`→`tiao` |
| 0 | `于乐调` | `yuletiao` | `yulediao` | `diao`→`tiao` |
| 0 | `周瑜调` | `zhouyutiao` | `zhouyudiao` | `diao`→`tiao` |
| 0 | `诸乐调` | `zhuletiao` | `zhulediao` | `diao`→`tiao` |
| 0 | `自动调车` | `zidongtiaoche` | `zidongdiaoche` | `diao`→`tiao` |
| 0 | `宗调露` | `zongtiaolu` | `zongdiaolu` | `diao`→`tiao` |

---

## 字 = `著` (392 rows)

- **Readings**: zhù (大陆主流: 著名/著作/著称/著有/著述/显著) · zhe/zhuó (繁体写法 — 繁体「著」=简体「着」助词或附着义)
- **pypinyin primary**: zhù (主流大陆)
- **Direction stats**:
  - `zhu` → `zhao` × 131
  - `zhu` → `zhuo` × 131
  - `zhu` → `zhe` × 130

### Default direction proposal: `del_wrong`

> 大陆 zhù 主流;繁体着字用法(意味著/穿著/跟著 = 简体「意味着/穿着/跟着」)要 exception 保留(用户可能在繁体环境 / 港台文本 / 引用 input).

### Exception draft — keep `zhe` reading  *(exception: `del_prim`)*

(63 words)

- `意味著` (freq=17720)
- `穿著` (freq=16814)
- `跟著` (freq=15492)
- `沿著` (freq=14738)
- `随著` (not in candidates.tsv)
- `看著` (not in candidates.tsv)
- `顺著` (not in candidates.tsv)
- `对著` (not in candidates.tsv)
- `靠著` (freq=13112)
- `拿著` (freq=12961)
- `拉著` (not in candidates.tsv)
- `拖著` (freq=7989)
- `推著` (not in candidates.tsv)
- `抱著` (freq=12358)
- `抓著` (not in candidates.tsv)
- `捧著` (freq=6660)
- `压著` (not in candidates.tsv)
- `想著` (not in candidates.tsv)
- `念著` (not in candidates.tsv)
- `盼著` (not in candidates.tsv)
- `等著` (not in candidates.tsv)
- `守著` (freq=8024)
- `站著` (not in candidates.tsv)
- `坐著` (not in candidates.tsv)
- `躺著` (not in candidates.tsv)
- `活著` (freq=12039)
- `死著` (not in candidates.tsv)
- `趴著` (not in candidates.tsv)
- `蹲著` (not in candidates.tsv)
- `跪著` (not in candidates.tsv)
- `趟著` (not in candidates.tsv)
- `托著` (not in candidates.tsv)
- `挂著` (not in candidates.tsv)
- `朝著` (freq=12293)
- `面著` (not in candidates.tsv)
- `背著` (not in candidates.tsv)
- `贴著` (not in candidates.tsv)
- `附著` (freq=10669)
- `环著` (not in candidates.tsv)
- `围著` (not in candidates.tsv)
- `听著` (not in candidates.tsv)
- `笑著` (not in candidates.tsv)
- `哭著` (not in candidates.tsv)
- `唱著` (not in candidates.tsv)
- `叫著` (not in candidates.tsv)
- `喊著` (freq=7877)
- `跑著` (not in candidates.tsv)
- `走著` (not in candidates.tsv)
- `飞著` (not in candidates.tsv)
- `活著的` (not in candidates.tsv)
- `写著` (not in candidates.tsv)
- `写著呢` (not in candidates.tsv)
- `等著瞧` (not in candidates.tsv)
- `看著办` (not in candidates.tsv)
- `急著` (not in candidates.tsv)
- `忙著` (not in candidates.tsv)
- `拿著走` (not in candidates.tsv)
- `带著走` (not in candidates.tsv)
- `跟著做` (not in candidates.tsv)
- `顺著来` (not in candidates.tsv)
- `凑著用` (not in candidates.tsv)
- `笑著说` (not in candidates.tsv)
- `哭著说` (not in candidates.tsv)

### Exception draft — keep `zhuo` reading  *(exception: `del_prim`)*

(4 words)

- `著重` (freq=15264)
- `著陆` (not in candidates.tsv)
- `附著` (freq=10669)
- `顯著` (not in candidates.tsv)

### Full word list (all 392 rows, sorted by freq desc)

| freq | word | typed_code | correct_code | dir |
|---|---|---|---|---|
| 33658 | `著名` | `zhaoming` | `zhuming` | `zhu`→`zhao` |
| 33658 | `著名` | `zheming` | `zhuming` | `zhu`→`zhe` |
| 33658 | `著名` | `zhuoming` | `zhuming` | `zhu`→`zhuo` |
| 23388 | `著作` | `zhaozuo` | `zhuzuo` | `zhu`→`zhao` |
| 23388 | `著作` | `zhezuo` | `zhuzuo` | `zhu`→`zhe` |
| 23388 | `著作` | `zhuozuo` | `zhuzuo` | `zhu`→`zhuo` |
| 18254 | `著称` | `zhaochen` | `zhucheng` | `zhu`→`zhao` |
| 18254 | `著称` | `zhaocheng` | `zhucheng` | `zhu`→`zhao` |
| 18254 | `著称` | `zhechen` | `zhucheng` | `zhu`→`zhe` |
| 18254 | `著称` | `zhecheng` | `zhucheng` | `zhu`→`zhe` |
| 18254 | `著称` | `zhuochen` | `zhucheng` | `zhu`→`zhuo` |
| 18254 | `著称` | `zhuocheng` | `zhucheng` | `zhu`→`zhuo` |
| 17720 | `意味著` | `yiweizhao` | `yiweizhu` | `zhu`→`zhao` |
| 17720 | `意味著` | `yiweizhe` | `yiweizhu` | `zhu`→`zhe` |
| 17720 | `意味著` | `yiweizhuo` | `yiweizhu` | `zhu`→`zhuo` |
| 16966 | `著有` | `zhaoyou` | `zhuyou` | `zhu`→`zhao` |
| 16966 | `著有` | `zheyou` | `zhuyou` | `zhu`→`zhe` |
| 16966 | `著有` | `zhuoyou` | `zhuyou` | `zhu`→`zhuo` |
| 16814 | `穿著` | `chuanzhao` | `chuanzhu` | `zhu`→`zhao` |
| 16814 | `穿著` | `chuanzhe` | `chuanzhu` | `zhu`→`zhe` |
| 16814 | `穿著` | `chuanzhuo` | `chuanzhu` | `zhu`→`zhuo` |
| 15492 | `跟著` | `genzhao` | `genzhu` | `zhu`→`zhao` |
| 15492 | `跟著` | `genzhe` | `genzhu` | `zhu`→`zhe` |
| 15492 | `跟著` | `genzhuo` | `genzhu` | `zhu`→`zhuo` |
| 15264 | `著重` | `zhaochong` | `zhuzhong` | `zhu`→`zhao` |
| 15264 | `著重` | `zhaozhong` | `zhuzhong` | `zhu`→`zhao` |
| 15264 | `著重` | `zhechong` | `zhuzhong` | `zhu`→`zhe` |
| 15264 | `著重` | `zhezhong` | `zhuzhong` | `zhu`→`zhe` |
| 15264 | `著重` | `zhuochong` | `zhuzhong` | `zhu`→`zhuo` |
| 15264 | `著重` | `zhuozhong` | `zhuzhong` | `zhu`→`zhuo` |
| 14789 | `著手` | `zhaoshou` | `zhushou` | `zhu`→`zhao` |
| 14789 | `著手` | `zheshou` | `zhushou` | `zhu`→`zhe` |
| 14789 | `著手` | `zhuoshou` | `zhushou` | `zhu`→`zhuo` |
| 14738 | `沿著` | `yanzhao` | `yanzhu` | `zhu`→`zhao` |
| 14738 | `沿著` | `yanzhe` | `yanzhu` | `zhu`→`zhe` |
| 14738 | `沿著` | `yanzhuo` | `yanzhu` | `zhu`→`zhuo` |
| 13390 | `藉著` | `jizhao` | `jizhu` | `zhu`→`zhao` |
| 13390 | `藉著` | `jizhe` | `jizhu` | `zhu`→`zhe` |
| 13390 | `藉著` | `jizhuo` | `jizhu` | `zhu`→`zhuo` |
| 13112 | `靠著` | `kaozhao` | `kaozhu` | `zhu`→`zhao` |
| 13112 | `靠著` | `kaozhe` | `kaozhu` | `zhu`→`zhe` |
| 13112 | `靠著` | `kaozhuo` | `kaozhu` | `zhu`→`zhuo` |
| 13046 | `著述` | `zhaoshu` | `zhushu` | `zhu`→`zhao` |
| 13046 | `著述` | `zheshu` | `zhushu` | `zhu`→`zhe` |
| 13046 | `著述` | `zhuoshu` | `zhushu` | `zhu`→`zhuo` |
| 12961 | `拿著` | `nazhao` | `nazhu` | `zhu`→`zhao` |
| 12961 | `拿著` | `nazhe` | `nazhu` | `zhu`→`zhe` |
| 12961 | `拿著` | `nazhuo` | `nazhu` | `zhu`→`zhuo` |
| 12439 | `著名作家` | `zhaomingzuojia` | `zhumingzuojia` | `zhu`→`zhao` |
| 12439 | `著名作家` | `zhemingzuojia` | `zhumingzuojia` | `zhu`→`zhe` |
| 12439 | `著名作家` | `zhuomingzuojia` | `zhumingzuojia` | `zhu`→`zhuo` |
| 12358 | `抱著` | `baozhao` | `baozhu` | `zhu`→`zhao` |
| 12358 | `抱著` | `baozhe` | `baozhu` | `zhu`→`zhe` |
| 12358 | `抱著` | `baozhuo` | `baozhu` | `zhu`→`zhuo` |
| 12293 | `朝著` | `chaozhao` | `chaozhu` | `zhu`→`zhao` |
| 12293 | `朝著` | `chaozhe` | `chaozhu` | `zhu`→`zhe` |
| 12293 | `朝著` | `chaozhuo` | `chaozhu` | `zhu`→`zhuo` |
| 12253 | `著作权` | `zhaozuoquan` | `zhuzuoquan` | `zhu`→`zhao` |
| 12253 | `著作权` | `zhezuoquan` | `zhuzuoquan` | `zhu`→`zhe` |
| 12253 | `著作权` | `zhuozuoquan` | `zhuzuoquan` | `zhu`→`zhuo` |
| 12218 | `著名景点` | `zhaomingjingdian` | `zhumingjingdian` | `zhu`→`zhao` |
| 12218 | `著名景点` | `zhemingjingdian` | `zhumingjingdian` | `zhu`→`zhe` |
| 12218 | `著名景点` | `zhuomingjingdian` | `zhumingjingdian` | `zhu`→`zhuo` |
| 12039 | `活著` | `huozhao` | `huozhu` | `zhu`→`zhao` |
| 12039 | `活著` | `huozhe` | `huozhu` | `zhu`→`zhe` |
| 12039 | `活著` | `huozhuo` | `huozhu` | `zhu`→`zhuo` |
| 11741 | `著书` | `zhaoshu` | `zhushu` | `zhu`→`zhao` |
| 11741 | `著书` | `zheshu` | `zhushu` | `zhu`→`zhe` |
| 11741 | `著书` | `zhuoshu` | `zhushu` | `zhu`→`zhuo` |
| 11527 | `著者` | `zhaozhe` | `zhuzhe` | `zhu`→`zhao` |
| 11527 | `著者` | `zhezhe` | `zhuzhe` | `zhu`→`zhe` |
| 11527 | `著者` | `zhuozhe` | `zhuzhe` | `zhu`→`zhuo` |
| 11237 | `冒著` | `maozhao` | `maozhu` | `zhu`→`zhao` |
| 11237 | `冒著` | `maozhe` | `maozhu` | `zhu`→`zhe` |
| 11237 | `冒著` | `maozhuo` | `maozhu` | `zhu`→`zhuo` |
| 11169 | `留著` | `liuzhao` | `liuzhu` | `zhu`→`zhao` |
| 11169 | `留著` | `liuzhe` | `liuzhu` | `zhu`→`zhe` |
| 11169 | `留著` | `liuzhuo` | `liuzhu` | `zhu`→`zhuo` |
| 11101 | `趁著` | `chenzhao` | `chenzhu` | `zhu`→`zhao` |
| 11101 | `趁著` | `chenzhe` | `chenzhu` | `zhu`→`zhe` |
| 11101 | `趁著` | `chenzhuo` | `chenzhu` | `zhu`→`zhuo` |
| 10730 | `著各` | `zhaoge` | `zhuge` | `zhu`→`zhao` |
| 10730 | `著各` | `zhuoge` | `zhuge` | `zhu`→`zhuo` |
| 10669 | `附著` | `fuzhao` | `fuzhu` | `zhu`→`zhao` |
| 10669 | `附著` | `fuzhe` | `fuzhu` | `zhu`→`zhe` |
| 10669 | `附著` | `fuzhuo` | `fuzhu` | `zhu`→`zhuo` |
| 10553 | `著相` | `zhaoxiang` | `zhuxiang` | `zhu`→`zhao` |
| 10553 | `著相` | `zhexiang` | `zhuxiang` | `zhu`→`zhe` |
| 10553 | `著相` | `zhuoxiang` | `zhuxiang` | `zhu`→`zhuo` |
| 10517 | `示著` | `shizhao` | `shizhu` | `zhu`→`zhao` |
| 10517 | `示著` | `shizhe` | `shizhu` | `zhu`→`zhe` |
| 10517 | `示著` | `shizhuo` | `shizhu` | `zhu`→`zhuo` |
| 10514 | `著名诗人` | `zhaomingshiren` | `zhumingshiren` | `zhu`→`zhao` |
| 10514 | `著名诗人` | `zhemingshiren` | `zhumingshiren` | `zhu`→`zhe` |
| 10514 | `著名诗人` | `zhuomingshiren` | `zhumingshiren` | `zhu`→`zhuo` |
| 9797 | `隔著` | `gezhao` | `gezhu` | `zhu`→`zhao` |
| 9797 | `隔著` | `gezhe` | `gezhu` | `zhu`→`zhe` |
| 9797 | `隔著` | `gezhuo` | `gezhu` | `zhu`→`zhuo` |
| 9710 | `著迷` | `zhaomi` | `zhumi` | `zhu`→`zhao` |
| 9710 | `著迷` | `zhemi` | `zhumi` | `zhu`→`zhe` |
| 9710 | `著迷` | `zhuomi` | `zhumi` | `zhu`→`zhuo` |
| 8987 | `握著` | `wozhao` | `wozhu` | `zhu`→`zhao` |
| 8987 | `握著` | `wozhe` | `wozhu` | `zhu`→`zhe` |
| 8987 | `握著` | `wozhuo` | `wozhu` | `zhu`→`zhuo` |
| 8974 | `经典著作` | `jingdianzhaozuo` | `jingdianzhuzuo` | `zhu`→`zhao` |
| 8974 | `经典著作` | `jingdianzhezuo` | `jingdianzhuzuo` | `zhu`→`zhe` |
| 8974 | `经典著作` | `jingdianzhuozuo` | `jingdianzhuzuo` | `zhu`→`zhuo` |
| 8716 | `重要著作` | `zhongyaozhaozuo` | `zhongyaozhuzuo` | `zhu`→`zhao` |
| 8716 | `重要著作` | `zhongyaozhezuo` | `zhongyaozhuzuo` | `zhu`→`zhe` |
| 8716 | `重要著作` | `zhongyaozhuozuo` | `zhongyaozhuzuo` | `zhu`→`zhuo` |
| 8249 | `藏著` | `cangzhao` | `cangzhu` | `zhu`→`zhao` |
| 8249 | `藏著` | `cangzhe` | `cangzhu` | `zhu`→`zhe` |
| 8249 | `藏著` | `cangzhuo` | `cangzhu` | `zhu`→`zhuo` |
| 8247 | `著父` | `zhaofu` | `zhufu` | `zhu`→`zhao` |
| 8247 | `著父` | `zhefu` | `zhufu` | `zhu`→`zhe` |
| 8247 | `著父` | `zhuofu` | `zhufu` | `zhu`→`zhuo` |
| 8202 | `含著` | `hanzhao` | `hanzhu` | `zhu`→`zhao` |
| 8202 | `含著` | `hanzhe` | `hanzhu` | `zhu`→`zhe` |
| 8202 | `含著` | `hanzhuo` | `hanzhu` | `zhu`→`zhuo` |
| 8156 | `即著` | `jizhao` | `jizhu` | `zhu`→`zhao` |
| 8156 | `即著` | `jizhe` | `jizhu` | `zhu`→`zhe` |
| 8156 | `即著` | `jizhuo` | `jizhu` | `zhu`→`zhuo` |
| 8041 | `披著` | `pizhao` | `pizhu` | `zhu`→`zhao` |
| 8041 | `披著` | `pizhe` | `pizhu` | `zhu`→`zhe` |
| 8041 | `披著` | `pizhuo` | `pizhu` | `zhu`→`zhuo` |
| 8024 | `守著` | `shouzhao` | `shouzhu` | `zhu`→`zhao` |
| 8024 | `守著` | `shouzhe` | `shouzhu` | `zhu`→`zhe` |
| 8024 | `守著` | `shouzhuo` | `shouzhu` | `zhu`→`zhuo` |
| 7989 | `拖著` | `tuozhao` | `tuozhu` | `zhu`→`zhao` |
| 7989 | `拖著` | `tuozhe` | `tuozhu` | `zhu`→`zhe` |
| 7989 | `拖著` | `tuozhuo` | `tuozhu` | `zhu`→`zhuo` |
| 7877 | `喊著` | `hanzhao` | `hanzhu` | `zhu`→`zhao` |
| 7877 | `喊著` | `hanzhe` | `hanzhu` | `zhu`→`zhe` |
| 7877 | `喊著` | `hanzhuo` | `hanzhu` | `zhu`→`zhuo` |
| 7838 | `著色` | `zhaose` | `zhuse` | `zhu`→`zhao` |
| 7838 | `著色` | `zhese` | `zhuse` | `zhu`→`zhe` |
| 7838 | `著色` | `zhuose` | `zhuse` | `zhu`→`zhuo` |
| 7789 | `按著` | `anzhao` | `anzhu` | `zhu`→`zhao` |
| 7789 | `按著` | `anzhe` | `anzhu` | `zhu`→`zhe` |
| 7789 | `按著` | `anzhuo` | `anzhu` | `zhu`→`zhuo` |
| 7661 | `著录` | `zhaolu` | `zhulu` | `zhu`→`zhao` |
| 7661 | `著录` | `zhelu` | `zhulu` | `zhu`→`zhe` |
| 7661 | `著录` | `zhuolu` | `zhulu` | `zhu`→`zhuo` |
| 7661 | `著墨` | `zhaomo` | `zhumo` | `zhu`→`zhao` |
| 7661 | `著墨` | `zhemo` | `zhumo` | `zhu`→`zhe` |
| 7661 | `著墨` | `zhuomo` | `zhumo` | `zhu`→`zhuo` |
| 7605 | `著名画家` | `zhaominghuajia` | `zhuminghuajia` | `zhu`→`zhao` |
| 7605 | `著名画家` | `zheminghuajia` | `zhuminghuajia` | `zhu`→`zhe` |
| 7605 | `著名画家` | `zhuominghuajia` | `zhuminghuajia` | `zhu`→`zhuo` |
| 7508 | `著名人士` | `zhaomingrenshi` | `zhumingrenshi` | `zhu`→`zhao` |
| 7508 | `著名人士` | `zhemingrenshi` | `zhumingrenshi` | `zhu`→`zhe` |
| 7508 | `著名人士` | `zhuomingrenshi` | `zhumingrenshi` | `zhu`→`zhuo` |
| 7482 | `著名演员` | `zhaomingyanyuan` | `zhumingyanyuan` | `zhu`→`zhao` |
| 7482 | `著名演员` | `zhemingyanyuan` | `zhumingyanyuan` | `zhu`→`zhe` |
| 7482 | `著名演员` | `zhuomingyanyuan` | `zhumingyanyuan` | `zhu`→`zhuo` |
| 7399 | `乘著` | `chengzhao` | `chengzhu` | `zhu`→`zhao` |
| 7399 | `乘著` | `chengzhe` | `chengzhu` | `zhu`→`zhe` |
| 7399 | `乘著` | `chengzhuo` | `chengzhu` | `zhu`→`zhuo` |
| 7399 | `黏著` | `nianzhao` | `nianzhu` | `zhu`→`zhao` |
| 7399 | `黏著` | `nianzhe` | `nianzhu` | `zhu`→`zhe` |
| 7399 | `黏著` | `nianzhuo` | `nianzhu` | `zhu`→`zhuo` |
| 7328 | `沉著` | `chenzhao` | `chenzhu` | `zhu`→`zhao` |
| 7328 | `沉著` | `chenzhe` | `chenzhu` | `zhu`→`zhe` |
| 7328 | `沉著` | `chenzhuo` | `chenzhu` | `zhu`→`zhuo` |
| 6936 | `循著` | `xunzhao` | `xunzhu` | `zhu`→`zhao` |
| 6936 | `循著` | `xunzhe` | `xunzhu` | `zhu`→`zhe` |
| 6936 | `循著` | `xunzhuo` | `xunzhu` | `zhu`→`zhuo` |
| 6920 | `扶著` | `fuzhao` | `fuzhu` | `zhu`→`zhao` |
| 6920 | `扶著` | `fuzhe` | `fuzhu` | `zhu`→`zhe` |
| 6920 | `扶著` | `fuzhuo` | `fuzhu` | `zhu`→`zhuo` |
| 6883 | `著身` | `zhaoshen` | `zhushen` | `zhu`→`zhao` |
| 6883 | `著身` | `zheshen` | `zhushen` | `zhu`→`zhe` |
| 6883 | `著身` | `zhuoshen` | `zhushen` | `zhu`→`zhuo` |
| 6756 | `著某` | `zhaomou` | `zhumou` | `zhu`→`zhao` |
| 6756 | `著某` | `zhemou` | `zhumou` | `zhu`→`zhe` |
| 6756 | `著某` | `zhuomou` | `zhumou` | `zhu`→`zhuo` |
| 6660 | `捧著` | `pengzhao` | `pengzhu` | `zhu`→`zhao` |
| 6660 | `捧著` | `pengzhe` | `pengzhu` | `zhu`→`zhe` |
| 6660 | `捧著` | `pengzhuo` | `pengzhu` | `zhu`→`zhuo` |
| 6660 | `学术著作` | `xueshuzhaozuo` | `xueshuzhuzuo` | `zhu`→`zhao` |
| 6660 | `学术著作` | `xueshuzhezuo` | `xueshuzhuzuo` | `zhu`→`zhe` |
| 6660 | `学术著作` | `xueshuzhuozuo` | `xueshuzhuzuo` | `zhu`→`zhuo` |
| 6655 | `著名品牌` | `zhaomingpinpai` | `zhumingpinpai` | `zhu`→`zhao` |
| 6655 | `著名品牌` | `zhemingpinpai` | `zhumingpinpai` | `zhu`→`zhe` |
| 6655 | `著名品牌` | `zhuomingpinpai` | `zhumingpinpai` | `zhu`→`zhuo` |
| 6504 | `著若` | `zhaoruo` | `zhuruo` | `zhu`→`zhao` |
| 6504 | `著若` | `zheruo` | `zhuruo` | `zhu`→`zhe` |
| 6504 | `著若` | `zhuoruo` | `zhuruo` | `zhu`→`zhuo` |
| 6362 | `杂著` | `zazhao` | `zazhu` | `zhu`→`zhao` |
| 6362 | `杂著` | `zazhe` | `zazhu` | `zhu`→`zhe` |
| 6362 | `杂著` | `zazhuo` | `zazhu` | `zhu`→`zhuo` |
| 6362 | `著伊` | `zhaoyi` | `zhuyi` | `zhu`→`zhao` |
| 6362 | `著伊` | `zheyi` | `zhuyi` | `zhu`→`zhe` |
| 6362 | `著伊` | `zhuoyi` | `zhuyi` | `zhu`→`zhuo` |
| 6344 | `固著` | `guzhao` | `guzhu` | `zhu`→`zhao` |
| 6344 | `固著` | `guzhe` | `guzhu` | `zhu`→`zhe` |
| 6344 | `固著` | `guzhuo` | `guzhu` | `zhu`→`zhuo` |
| 6344 | `述著` | `shuzhao` | `shuzhu` | `zhu`→`zhao` |
| 6344 | `述著` | `shuzhe` | `shuzhu` | `zhu`→`zhe` |
| 6344 | `述著` | `shuzhuo` | `shuzhu` | `zhu`→`zhuo` |
| 6044 | `哲学著作` | `zhexuezhaozuo` | `zhexuezhuzuo` | `zhu`→`zhao` |
| 6044 | `哲学著作` | `zhexuezhezuo` | `zhexuezhuzuo` | `zhu`→`zhe` |
| 6044 | `哲学著作` | `zhexuezhuozuo` | `zhexuezhuzuo` | `zhu`→`zhuo` |
| 5974 | `科学著作` | `kexuezhaozuo` | `kexuezhuzuo` | `zhu`→`zhao` |
| 5974 | `科学著作` | `kexuezhezuo` | `kexuezhuzuo` | `zhu`→`zhe` |
| 5974 | `科学著作` | `kexuezhuozuo` | `kexuezhuzuo` | `zhu`→`zhuo` |
| 5835 | `覆著` | `fuzhao` | `fuzhu` | `zhu`→`zhao` |
| 5835 | `覆著` | `fuzhe` | `fuzhu` | `zhu`→`zhe` |
| 5835 | `覆著` | `fuzhuo` | `fuzhu` | `zhu`→`zhuo` |
| 5835 | `篇著` | `pianzhao` | `pianzhu` | `zhu`→`zhao` |
| 5835 | `篇著` | `pianzhe` | `pianzhu` | `zhu`→`zhe` |
| 5835 | `篇著` | `pianzhuo` | `pianzhu` | `zhu`→`zhuo` |
| 5835 | `著作者` | `zhaozuozhe` | `zhuzuozhe` | `zhu`→`zhao` |
| 5835 | `著作者` | `zhezuozhe` | `zhuzuozhe` | `zhu`→`zhe` |
| 5835 | `著作者` | `zhuozuozhe` | `zhuzuozhe` | `zhu`→`zhuo` |
| 5686 | `遗著` | `yizhao` | `yizhu` | `zhu`→`zhao` |
| 5686 | `遗著` | `yizhe` | `yizhu` | `zhu`→`zhe` |
| 5686 | `遗著` | `yizhuo` | `yizhu` | `zhu`→`zhuo` |
| 5354 | `著固` | `zhaogu` | `zhugu` | `zhu`→`zhao` |
| 5354 | `著固` | `zhegu` | `zhugu` | `zhu`→`zhe` |
| 5354 | `著固` | `zhuogu` | `zhugu` | `zhu`→`zhuo` |
| 5167 | `仗著` | `zhangzhao` | `zhangzhu` | `zhu`→`zhao` |
| 5167 | `仗著` | `zhangzhe` | `zhangzhu` | `zhu`→`zhe` |
| 5167 | `仗著` | `zhangzhuo` | `zhangzhu` | `zhu`→`zhuo` |
| 4804 | `著名商标` | `zhaomingshangbiao` | `zhumingshangbiao` | `zhu`→`zhao` |
| 4804 | `著名商标` | `zhemingshangbiao` | `zhumingshangbiao` | `zhu`→`zhe` |
| 4804 | `著名商标` | `zhuomingshangbiao` | `zhumingshangbiao` | `zhu`→`zhuo` |
| 4734 | `可穿著` | `kechuanzhao` | `kechuanzhu` | `zhu`→`zhao` |
| 4734 | `可穿著` | `kechuanzhe` | `kechuanzhu` | `zhu`→`zhe` |
| 4734 | `可穿著` | `kechuanzhuo` | `kechuanzhu` | `zhu`→`zhuo` |
| 4734 | `士著` | `shizhao` | `shizhu` | `zhu`→`zhao` |
| 4734 | `士著` | `shizhe` | `shizhu` | `zhu`→`zhe` |
| 4734 | `士著` | `shizhuo` | `shizhu` | `zhu`→`zhuo` |
| 4640 | `著合` | `zhaohe` | `zhuhe` | `zhu`→`zhao` |
| 4640 | `著合` | `zhehe` | `zhuhe` | `zhu`→`zhe` |
| 4640 | `著合` | `zhuohe` | `zhuhe` | `zhu`→`zhuo` |
| 2987 | `斜著` | `xiezhao` | `xiezhu` | `zhu`→`zhao` |
| 2987 | `斜著` | `xiezhe` | `xiezhu` | `zhu`→`zhe` |
| 2987 | `斜著` | `xiezhuo` | `xiezhu` | `zhu`→`zhuo` |
| 2367 | `紧接著` | `jinjiezhao` | `jinjiezhu` | `zhu`→`zhao` |
| 2367 | `紧接著` | `jinjiezhe` | `jinjiezhu` | `zhu`→`zhe` |
| 2367 | `紧接著` | `jinjiezhuo` | `jinjiezhu` | `zhu`→`zhuo` |
| 2367 | `馨著` | `xinzhao` | `xinzhu` | `zhu`→`zhao` |
| 2367 | `馨著` | `xinzhe` | `xinzhu` | `zhu`→`zhe` |
| 2367 | `馨著` | `xinzhuo` | `xinzhu` | `zhu`→`zhuo` |
| 2367 | `著译` | `zhaoyi` | `zhuyi` | `zhu`→`zhao` |
| 2367 | `著译` | `zheyi` | `zhuyi` | `zhu`→`zhe` |
| 2367 | `著译` | `zhuoyi` | `zhuyi` | `zhu`→`zhuo` |
| 1493 | `著名权` | `zhaomingquan` | `zhumingquan` | `zhu`→`zhao` |
| 1493 | `著名权` | `zhemingquan` | `zhumingquan` | `zhu`→`zhe` |
| 1493 | `著名权` | `zhuomingquan` | `zhumingquan` | `zhu`→`zhuo` |
| 1493 | `著作权人` | `zhaozuoquanren` | `zhuzuoquanren` | `zhu`→`zhao` |
| 1493 | `著作权人` | `zhezuoquanren` | `zhuzuoquanren` | `zhu`→`zhe` |
| 1493 | `著作权人` | `zhuozuoquanren` | `zhuzuoquanren` | `zhu`→`zhuo` |
| 0 | `班固著` | `banguzhao` | `banguzhu` | `zhu`→`zhao` |
| 0 | `班固著` | `banguzhe` | `banguzhu` | `zhu`→`zhe` |
| 0 | `班固著` | `banguzhuo` | `banguzhu` | `zhu`→`zhuo` |
| 0 | `贝祖著` | `beizuzhao` | `beizuzhu` | `zhu`→`zhao` |
| 0 | `贝祖著` | `beizuzhe` | `beizuzhu` | `zhu`→`zhe` |
| 0 | `贝祖著` | `beizuzhuo` | `beizuzhu` | `zhu`→`zhuo` |
| 0 | `标志著` | `biaozhizhao` | `biaozhizhu` | `zhu`→`zhao` |
| 0 | `标志著` | `biaozhizhe` | `biaozhizhu` | `zhu`→`zhe` |
| 0 | `标志著` | `biaozhizhuo` | `biaozhizhu` | `zhu`→`zhuo` |
| 0 | `伯赞著` | `bozanzhao` | `bozanzhu` | `zhu`→`zhao` |
| 0 | `伯赞著` | `bozanzhe` | `bozanzhu` | `zhu`→`zhe` |
| 0 | `伯赞著` | `bozanzhuo` | `bozanzhu` | `zhu`→`zhuo` |
| 0 | `吃衣著饭` | `chiyizhaofan` | `chiyizhufan` | `zhu`→`zhao` |
| 0 | `吃衣著饭` | `chiyizhefan` | `chiyizhufan` | `zhu`→`zhe` |
| 0 | `吃衣著饭` | `chiyizhuofan` | `chiyizhufan` | `zhu`→`zhuo` |
| 0 | `充满著` | `chongmanzhao` | `chongmanzhu` | `zhu`→`zhao` |
| 0 | `充满著` | `chongmanzhe` | `chongmanzhu` | `zhu`→`zhe` |
| 0 | `充满著` | `chongmanzhuo` | `chongmanzhu` | `zhu`→`zhuo` |
| 0 | `戴震著` | `daizhenzhao` | `daizhenzhu` | `zhu`→`zhao` |
| 0 | `戴震著` | `daizhenzhe` | `daizhenzhu` | `zhu`→`zhe` |
| 0 | `戴震著` | `daizhenzhuo` | `daizhenzhu` | `zhu`→`zhuo` |
| 0 | `德特著` | `detezhao` | `detezhu` | `zhu`→`zhao` |
| 0 | `德特著` | `detezhe` | `detezhu` | `zhu`→`zhe` |
| 0 | `德特著` | `detezhuo` | `detezhu` | `zhu`→`zhuo` |
| 0 | `恩威并著` | `enweibingzhao` | `enweibingzhu` | `zhu`→`zhao` |
| 0 | `恩威并著` | `enweibingzhe` | `enweibingzhu` | `zhu`→`zhe` |
| 0 | `恩威并著` | `enweibingzhuo` | `enweibingzhu` | `zhu`→`zhuo` |
| 0 | `范著` | `fanzhao` | `fanzhu` | `zhu`→`zhao` |
| 0 | `范著` | `fanzhe` | `fanzhu` | `zhu`→`zhe` |
| 0 | `范著` | `fanzhuo` | `fanzhu` | `zhu`→`zhuo` |
| 0 | `费尼著` | `feinizhao` | `feinizhu` | `zhu`→`zhao` |
| 0 | `费尼著` | `feinizhe` | `feinizhu` | `zhu`→`zhe` |
| 0 | `费尼著` | `feinizhuo` | `feinizhu` | `zhu`→`zhuo` |
| 0 | `费信著` | `feixinzhao` | `feixinzhu` | `zhu`→`zhao` |
| 0 | `费信著` | `feixinzhe` | `feixinzhu` | `zhu`→`zhe` |
| 0 | `费信著` | `feixinzhuo` | `feixinzhu` | `zhu`→`zhuo` |
| 0 | `格林著` | `gelinzhao` | `gelinzhu` | `zhu`→`zhao` |
| 0 | `格林著` | `gelinzhe` | `gelinzhu` | `zhu`→`zhe` |
| 0 | `格林著` | `gelinzhuo` | `gelinzhu` | `zhu`→`zhuo` |
| 0 | `巩珍著` | `gongzhenzhao` | `gongzhenzhu` | `zhu`→`zhao` |
| 0 | `巩珍著` | `gongzhenzhe` | `gongzhenzhu` | `zhu`→`zhe` |
| 0 | `巩珍著` | `gongzhenzhuo` | `gongzhenzhu` | `zhu`→`zhuo` |
| 0 | `桂馥著` | `guifuzhao` | `guifuzhu` | `zhu`→`zhao` |
| 0 | `桂馥著` | `guifuzhe` | `guifuzhu` | `zhu`→`zhe` |
| 0 | `桂馥著` | `guifuzhuo` | `guifuzhu` | `zhu`→`zhuo` |
| 0 | `何休著` | `hexiuzhao` | `hexiuzhu` | `zhu`→`zhao` |
| 0 | `何休著` | `hexiuzhe` | `hexiuzhu` | `zhu`→`zhe` |
| 0 | `何休著` | `hexiuzhuo` | `hexiuzhu` | `zhu`→`zhuo` |
| 0 | `李荣著` | `lirongzhao` | `lirongzhu` | `zhu`→`zhao` |
| 0 | `李荣著` | `lirongzhe` | `lirongzhu` | `zhu`→`zhe` |
| 0 | `李荣著` | `lirongzhuo` | `lirongzhu` | `zhu`→`zhuo` |
| 0 | `刘劭著` | `liushaozhao` | `liushaozhu` | `zhu`→`zhao` |
| 0 | `刘劭著` | `liushaozhe` | `liushaozhu` | `zhu`→`zhe` |
| 0 | `刘劭著` | `liushaozhuo` | `liushaozhu` | `zhu`→`zhuo` |
| 0 | `李冶著` | `liyezhao` | `liyezhu` | `zhu`→`zhao` |
| 0 | `李冶著` | `liyezhe` | `liyezhu` | `zhu`→`zhe` |
| 0 | `李冶著` | `liyezhuo` | `liyezhu` | `zhu`→`zhuo` |
| 0 | `李玉著` | `liyuzhao` | `liyuzhu` | `zhu`→`zhao` |
| 0 | `李玉著` | `liyuzhe` | `liyuzhu` | `zhu`→`zhe` |
| 0 | `李玉著` | `liyuzhuo` | `liyuzhu` | `zhu`→`zhuo` |
| 0 | `龙树著` | `longshuzhao` | `longshuzhu` | `zhu`→`zhao` |
| 0 | `龙树著` | `longshuzhe` | `longshuzhu` | `zhu`→`zhe` |
| 0 | `龙树著` | `longshuzhuo` | `longshuzhu` | `zhu`→`zhuo` |
| 0 | `马欢著` | `mahuanzhao` | `mahuanzhu` | `zhu`→`zhao` |
| 0 | `马欢著` | `mahuanzhe` | `mahuanzhu` | `zhu`→`zhe` |
| 0 | `马欢著` | `mahuanzhuo` | `mahuanzhu` | `zhu`→`zhuo` |
| 0 | `马列著作` | `maliezhaozuo` | `maliezhuzuo` | `zhu`→`zhao` |
| 0 | `马列著作` | `maliezhezuo` | `maliezhuzuo` | `zhu`→`zhe` |
| 0 | `马列著作` | `maliezhuozuo` | `maliezhuzuo` | `zhu`→`zhuo` |
| 0 | `马鸣著` | `mamingzhao` | `mamingzhu` | `zhu`→`zhao` |
| 0 | `马鸣著` | `mamingzhe` | `mamingzhu` | `zhu`→`zhe` |
| 0 | `马鸣著` | `mamingzhuo` | `mamingzhu` | `zhu`→`zhuo` |
| 0 | `毛毛著` | `maomaozhao` | `maomaozhu` | `zhu`→`zhao` |
| 0 | `毛毛著` | `maomaozhe` | `maomaozhu` | `zhu`→`zhe` |
| 0 | `毛毛著` | `maomaozhuo` | `maomaozhu` | `zhu`→`zhuo` |
| 0 | `马士著` | `mashizhao` | `mashizhu` | `zhu`→`zhao` |
| 0 | `马士著` | `mashizhe` | `mashizhu` | `zhu`→`zhe` |
| 0 | `马士著` | `mashizhuo` | `mashizhu` | `zhu`→`zhuo` |
| 0 | `莫斯著` | `mosizhao` | `mosizhu` | `zhu`→`zhao` |
| 0 | `莫斯著` | `mosizhe` | `mosizhu` | `zhu`→`zhe` |
| 0 | `莫斯著` | `mosizhuo` | `mosizhu` | `zhu`→`zhuo` |
| 0 | `普勒著` | `puleizhao` | `puleizhu` | `zhu`→`zhao` |
| 0 | `普勒著` | `puleizhe` | `puleizhu` | `zhu`→`zhe` |
| 0 | `普勒著` | `puleizhuo` | `puleizhu` | `zhu`→`zhuo` |
| 0 | `齐人著` | `qirenzhao` | `qirenzhu` | `zhu`→`zhao` |
| 0 | `齐人著` | `qirenzhe` | `qirenzhu` | `zhu`→`zhe` |
| 0 | `齐人著` | `qirenzhuo` | `qirenzhu` | `zhu`→`zhuo` |
| 0 | `宋江著` | `songjiangzhao` | `songjiangzhu` | `zhu`→`zhao` |
| 0 | `宋江著` | `songjiangzhe` | `songjiangzhu` | `zhu`→`zhe` |
| 0 | `宋江著` | `songjiangzhuo` | `songjiangzhu` | `zhu`→`zhuo` |
| 0 | `童跟著` | `tonggenzhao` | `tonggenzhu` | `zhu`→`zhao` |
| 0 | `童跟著` | `tonggenzhe` | `tonggenzhu` | `zhu`→`zhe` |
| 0 | `童跟著` | `tonggenzhuo` | `tonggenzhu` | `zhu`→`zhuo` |
| 0 | `王桢著` | `wangzhenzhao` | `wangzhenzhu` | `zhu`→`zhao` |
| 0 | `王桢著` | `wangzhenzhe` | `wangzhenzhu` | `zhu`→`zhe` |
| 0 | `王桢著` | `wangzhenzhuo` | `wangzhenzhu` | `zhu`→`zhuo` |
| 0 | `韦达著` | `weidazhao` | `weidazhu` | `zhu`→`zhao` |
| 0 | `韦达著` | `weidazhe` | `weidazhu` | `zhu`→`zhe` |
| 0 | `韦达著` | `weidazhuo` | `weidazhu` | `zhu`→`zhuo` |
| 0 | `吴炳著` | `wubingzhao` | `wubingzhu` | `zhu`→`zhao` |
| 0 | `吴炳著` | `wubingzhe` | `wubingzhu` | `zhu`→`zhe` |
| 0 | `吴炳著` | `wubingzhuo` | `wubingzhu` | `zhu`→`zhuo` |
| 0 | `乌斯著` | `wusizhao` | `wusizhu` | `zhu`→`zhao` |
| 0 | `乌斯著` | `wusizhe` | `wusizhu` | `zhu`→`zhe` |
| 0 | `乌斯著` | `wusizhuo` | `wusizhu` | `zhu`→`zhuo` |
| 0 | `许慎著` | `xushenzhao` | `xushenzhu` | `zhu`→`zhao` |
| 0 | `许慎著` | `xushenzhe` | `xushenzhu` | `zhu`→`zhe` |
| 0 | `许慎著` | `xushenzhuo` | `xushenzhu` | `zhu`→`zhuo` |
| 0 | `扬雄著` | `yangxiongzhao` | `yangxiongzhu` | `zhu`→`zhao` |
| 0 | `扬雄著` | `yangxiongzhe` | `yangxiongzhu` | `zhu`→`zhe` |
| 0 | `扬雄著` | `yangxiongzhuo` | `yangxiongzhu` | `zhu`→`zhuo` |
| 0 | `严衍著` | `yanyanzhao` | `yanyanzhu` | `zhu`→`zhao` |
| 0 | `严衍著` | `yanyanzhe` | `yanyanzhu` | `zhu`→`zhe` |
| 0 | `严衍著` | `yanyanzhuo` | `yanyanzhu` | `zhu`→`zhuo` |
| 0 | `著录规则` | `zhaoluguize` | `zhuluguize` | `zhu`→`zhao` |
| 0 | `著录规则` | `zheluguize` | `zhuluguize` | `zhu`→`zhe` |
| 0 | `著录规则` | `zhuoluguize` | `zhuluguize` | `zhu`→`zhuo` |
| 0 | `著实` | `zhaoshi` | `zhushi` | `zhu`→`zhao` |
| 0 | `著实` | `zheshi` | `zhushi` | `zhu`→`zhe` |
| 0 | `著实` | `zhuoshi` | `zhushi` | `zhu`→`zhuo` |
| 0 | `著者目录` | `zhaozhemulu` | `zhuzhemulu` | `zhu`→`zhao` |
| 0 | `著者目录` | `zhezhemulu` | `zhuzhemulu` | `zhu`→`zhe` |
| 0 | `著者目录` | `zhuozhemulu` | `zhuzhemulu` | `zhu`→`zhuo` |
| 0 | `著者索引` | `zhaozhesuoyin` | `zhuzhesuoyin` | `zhu`→`zhao` |
| 0 | `著者索引` | `zhezhesuoyin` | `zhuzhesuoyin` | `zhu`→`zhe` |
| 0 | `著者索引` | `zhuozhesuoyin` | `zhuzhesuoyin` | `zhu`→`zhuo` |
| 0 | `著作史` | `zhaozuoshi` | `zhuzuoshi` | `zhu`→`zhao` |
| 0 | `著作史` | `zhezuoshi` | `zhuzuoshi` | `zhu`→`zhe` |
| 0 | `著作史` | `zhuozuoshi` | `zhuzuoshi` | `zhu`→`zhuo` |
| 0 | `中国音乐著作权协会` | `zhongguoyinyuezhaozuoquanxiehui` | `zhongguoyinyuezhuzuoquanxiehui` | `zhu`→`zhao` |
| 0 | `中国音乐著作权协会` | `zhongguoyinyuezhezuoquanxiehui` | `zhongguoyinyuezhuzuoquanxiehui` | `zhu`→`zhe` |
| 0 | `中国音乐著作权协会` | `zhongguoyinyuezhuozuoquanxiehui` | `zhongguoyinyuezhuzuoquanxiehui` | `zhu`→`zhuo` |
| 0 | `子世著` | `zishizhao` | `zishizhu` | `zhu`→`zhao` |
| 0 | `子世著` | `zishizhe` | `zishizhu` | `zhu`→`zhe` |
| 0 | `子世著` | `zishizhuo` | `zishizhu` | `zhu`→`zhuo` |
| 0 | `邹容著` | `zourongzhao` | `zourongzhu` | `zhu`→`zhao` |
| 0 | `邹容著` | `zourongzhe` | `zourongzhu` | `zhu`→`zhe` |
| 0 | `邹容著` | `zourongzhuo` | `zourongzhu` | `zhu`→`zhuo` |

---

## Cumulative summary

- Total polyphone-dup rows across 4 chars: **1686**
  - `着`: 878 rows
  - `血`: 187 rows
  - `调`: 229 rows
  - `著`: 392 rows

## Next steps

1. **User audit (in-session)**: review each char section, finalize default + exception lists.
2. **Write verified RULES**: I add finalized triples to `batch2_hard_verdict.py` RULES dict.
3. **Apply**: run `python batch2_hard_verdict.py` → produces `batch2_hard_to_delete.tsv` → `apply_sweep.py` deletes rows from `pinyin.dict` source.
4. **Regen**: `pinyin.dict` + `words.idf` blobs regenerated (per `[[feedback-polish-bundle-regen-blobs]]`).
5. **Verify**: regression tests for representative kept/deleted cases + `make eval` ±2pp + 宪法门 12/12.
6. **Commit per char**: one `polish/phase5-batch2-hard-<char>` branch + `--no-ff` merge develop.
