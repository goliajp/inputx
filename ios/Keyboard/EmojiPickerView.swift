import UIKit

/// Inputx emoji picker (item 90 — v2 standard tier).
///
/// Layout, top-to-bottom:
///   - UICollectionView grid, one section per category, vertically scrollable.
///   - Tab strip — horizontal row of category-glyph buttons; tap jumps the
///     grid to the corresponding section.
///   - Bottom row — `[ABC ........... ⌫]` so the user can leave the picker
///     and erase without round-tripping back through the letter layer.
///
/// Recent emojis are surfaced as a synthetic first section (`hasRecent`)
/// when `RecentEmojis.load()` returns non-empty. The tab strip mirrors this
/// — a "clock" tab appears on the left whenever recent is non-empty.
///
/// Cell selection records to `RecentEmojis` and forwards via `onEmojiPicked`;
/// the host (KeyboardViewController) does the actual `textDocumentProxy
/// .insertText`. Picker holds no engine references — clean separation from
/// the IME letter/symbol layers.
final class EmojiPickerView: UIView {
    var onEmojiPicked: ((String) -> Void)?
    var onBackspace: (() -> Void)?
    var onBackToLetters: (() -> Void)?

    private var collection: UICollectionView!
    private let tabsScroll = UIScrollView()
    private let tabsStack = UIStackView()
    private let bottomRow = UIView()
    private var tabButtons: [UIButton] = []

    /// Section payload — pairs the category metadata with the visible emojis
    /// (for the static categories this is `EmojiCategory.emojis` verbatim;
    /// for the synthetic Recent section it's `RecentEmojis.load()`).
    private var sections: [(EmojiCategory, [String])] = []
    private var hasRecent: Bool = false
    /// Currently-selected tab index. Used by `updateTabSelection` to color
    /// the active glyph; updated either from explicit tab-tap OR from
    /// scrollViewDidScroll observing which section's first cell is on-screen.
    private var selectedTabIndex: Int = 0
    /// Suppress scroll-driven tab updates while a tab tap is animating the
    /// collection (otherwise the in-flight intermediate sections briefly
    /// re-highlight before settling on the target).
    private var ignoreScrollUpdates: Bool = false

    init() {
        super.init(frame: .zero)
        setup()
        reload()
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) not used") }

    private func setup() {
        backgroundColor = UIColor { trait in
            trait.userInterfaceStyle == .dark
                ? UIColor(white: 0.13, alpha: 1.0)
                : UIColor(white: 0.83, alpha: 1.0)
        }

        // ----- collection view -----
        let layout = UICollectionViewFlowLayout()
        layout.itemSize = CGSize(width: 38, height: 38)
        layout.minimumInteritemSpacing = 2
        layout.minimumLineSpacing = 4
        layout.sectionInset = UIEdgeInsets(top: 6, left: 6, bottom: 6, right: 6)
        collection = UICollectionView(frame: .zero, collectionViewLayout: layout)
        collection.translatesAutoresizingMaskIntoConstraints = false
        collection.backgroundColor = .clear
        collection.dataSource = self
        collection.delegate = self
        collection.register(EmojiCell.self, forCellWithReuseIdentifier: "cell")
        collection.alwaysBounceVertical = true
        addSubview(collection)

        // ----- tabs -----
        tabsScroll.translatesAutoresizingMaskIntoConstraints = false
        tabsScroll.showsHorizontalScrollIndicator = false
        addSubview(tabsScroll)

        tabsStack.axis = .horizontal
        tabsStack.alignment = .center
        tabsStack.distribution = .equalSpacing
        tabsStack.spacing = 12
        tabsStack.translatesAutoresizingMaskIntoConstraints = false
        tabsScroll.addSubview(tabsStack)

        // ----- bottom row: ABC + spacer + ⌫ -----
        bottomRow.translatesAutoresizingMaskIntoConstraints = false
        addSubview(bottomRow)

        let abc = KeyButton(title: "ABC", style: .modifier)
        abc.translatesAutoresizingMaskIntoConstraints = false
        abc.onTap = { [weak self] in self?.onBackToLetters?() }
        bottomRow.addSubview(abc)

        let backspace = KeyButton(
            systemImage: "delete.left",
            accessibilityLabel: "Backspace",
            style: .modifier
        )
        backspace.translatesAutoresizingMaskIntoConstraints = false
        backspace.onTap = { [weak self] in self?.onBackspace?() }
        bottomRow.addSubview(backspace)

        NSLayoutConstraint.activate([
            // Collection — top half of the picker.
            collection.topAnchor.constraint(equalTo: topAnchor),
            collection.leadingAnchor.constraint(equalTo: leadingAnchor),
            collection.trailingAnchor.constraint(equalTo: trailingAnchor),

            // Tabs strip — 40pt tall, just above the bottom row.
            tabsScroll.topAnchor.constraint(equalTo: collection.bottomAnchor),
            tabsScroll.leadingAnchor.constraint(equalTo: leadingAnchor),
            tabsScroll.trailingAnchor.constraint(equalTo: trailingAnchor),
            tabsScroll.heightAnchor.constraint(equalToConstant: 40),

            tabsStack.topAnchor.constraint(equalTo: tabsScroll.topAnchor),
            tabsStack.bottomAnchor.constraint(equalTo: tabsScroll.bottomAnchor),
            tabsStack.leadingAnchor.constraint(equalTo: tabsScroll.leadingAnchor, constant: 12),
            tabsStack.trailingAnchor.constraint(equalTo: tabsScroll.trailingAnchor, constant: -12),
            tabsStack.heightAnchor.constraint(equalTo: tabsScroll.heightAnchor),

            // Bottom row — 44pt, fills the bottom edge.
            bottomRow.topAnchor.constraint(equalTo: tabsScroll.bottomAnchor),
            bottomRow.leadingAnchor.constraint(equalTo: leadingAnchor),
            bottomRow.trailingAnchor.constraint(equalTo: trailingAnchor),
            bottomRow.bottomAnchor.constraint(equalTo: bottomAnchor),
            bottomRow.heightAnchor.constraint(equalToConstant: 44),

            abc.leadingAnchor.constraint(equalTo: bottomRow.leadingAnchor, constant: 4),
            abc.centerYAnchor.constraint(equalTo: bottomRow.centerYAnchor),
            abc.widthAnchor.constraint(equalToConstant: 64),
            abc.heightAnchor.constraint(equalToConstant: 36),

            backspace.trailingAnchor.constraint(equalTo: bottomRow.trailingAnchor, constant: -4),
            backspace.centerYAnchor.constraint(equalTo: bottomRow.centerYAnchor),
            backspace.widthAnchor.constraint(equalToConstant: 64),
            backspace.heightAnchor.constraint(equalToConstant: 36),
        ])
    }

    /// Rebuild section list + tab buttons. Cheap — call whenever recent
    /// might have changed (e.g., after picking an emoji) or on first show.
    func reload() {
        sections.removeAll()
        let recent = RecentEmojis.load()
        if !recent.isEmpty {
            hasRecent = true
            let cat = EmojiCategory(
                name: "Recent",
                tabIcon: "clock",
                tabIconIsSymbol: true,
                emojis: recent
            )
            sections.append((cat, recent))
        } else {
            hasRecent = false
        }
        for cat in EmojiData.categories {
            sections.append((cat, cat.emojis))
        }

        rebuildTabs()
        collection.reloadData()
        if selectedTabIndex >= sections.count {
            selectedTabIndex = 0
        }
        updateTabSelection()
    }

    private func rebuildTabs() {
        for v in tabsStack.arrangedSubviews {
            tabsStack.removeArrangedSubview(v)
            v.removeFromSuperview()
        }
        tabButtons.removeAll()
        for (i, (cat, _)) in sections.enumerated() {
            let btn = UIButton(type: .system)
            btn.tag = i
            btn.translatesAutoresizingMaskIntoConstraints = false
            btn.widthAnchor.constraint(equalToConstant: 28).isActive = true
            btn.heightAnchor.constraint(equalToConstant: 28).isActive = true
            if cat.tabIconIsSymbol {
                let cfg = UIImage.SymbolConfiguration(pointSize: 18, weight: .regular)
                btn.setImage(UIImage(systemName: cat.tabIcon, withConfiguration: cfg), for: .normal)
                btn.tintColor = .secondaryLabel
            } else {
                btn.setTitle(cat.tabIcon, for: .normal)
                btn.titleLabel?.font = .systemFont(ofSize: 20)
                btn.setTitleColor(.secondaryLabel, for: .normal)
            }
            btn.addTarget(self, action: #selector(tabTapped(_:)), for: .touchUpInside)
            btn.accessibilityLabel = cat.name
            tabsStack.addArrangedSubview(btn)
            tabButtons.append(btn)
        }
    }

    @objc private func tabTapped(_ sender: UIButton) {
        let i = sender.tag
        guard i < sections.count else { return }
        selectedTabIndex = i
        updateTabSelection()
        // Scroll to the first cell of the section. Use `.top` so the section
        // sits at the visible top; `.centeredVertically` would jitter for
        // short sections.
        ignoreScrollUpdates = true
        let path = IndexPath(item: 0, section: i)
        if collection.numberOfItems(inSection: i) > 0 {
            collection.scrollToItem(at: path, at: .top, animated: true)
        }
        // Clear suppression after the scroll animation settles.
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.35) { [weak self] in
            self?.ignoreScrollUpdates = false
        }
    }

    private func updateTabSelection() {
        for (i, btn) in tabButtons.enumerated() {
            let active = (i == selectedTabIndex)
            btn.tintColor = active ? .label : .secondaryLabel
            if !sections[i].0.tabIconIsSymbol {
                btn.setTitleColor(active ? .label : .secondaryLabel, for: .normal)
            }
            // Subtle bg highlight on the active tab — matches iOS feel.
            btn.backgroundColor = active
                ? UIColor.label.withAlphaComponent(0.08)
                : .clear
            btn.layer.cornerRadius = 6
        }
    }
}

// MARK: - UICollectionViewDataSource / Delegate

extension EmojiPickerView: UICollectionViewDataSource, UICollectionViewDelegate {
    func numberOfSections(in collectionView: UICollectionView) -> Int {
        return sections.count
    }
    func collectionView(_ cv: UICollectionView, numberOfItemsInSection section: Int) -> Int {
        return sections[section].1.count
    }
    func collectionView(_ cv: UICollectionView, cellForItemAt indexPath: IndexPath) -> UICollectionViewCell {
        let cell = cv.dequeueReusableCell(withReuseIdentifier: "cell", for: indexPath) as! EmojiCell
        cell.label.text = sections[indexPath.section].1[indexPath.item]
        return cell
    }
    func collectionView(_ cv: UICollectionView, didSelectItemAt indexPath: IndexPath) {
        let emoji = sections[indexPath.section].1[indexPath.item]
        RecentEmojis.record(emoji)
        UIImpactFeedbackGenerator(style: .light).impactOccurred()
        onEmojiPicked?(emoji)
        // Recent section content just changed — refresh it without a full
        // reload (full reload would scroll the user back to the top).
        if hasRecent {
            let updated = RecentEmojis.load()
            sections[0] = (sections[0].0, updated)
            cv.reloadSections(IndexSet(integer: 0))
        }
    }

    func scrollViewDidScroll(_ scrollView: UIScrollView) {
        guard scrollView === collection, !ignoreScrollUpdates else { return }
        // Find the topmost visible section. visibleIndexPaths order isn't
        // guaranteed, so min-by-section is the safe pick.
        guard let topPath = collection.indexPathsForVisibleItems
            .min(by: { $0.section < $1.section || ($0.section == $1.section && $0.item < $1.item) })
        else { return }
        if topPath.section != selectedTabIndex {
            selectedTabIndex = topPath.section
            updateTabSelection()
        }
    }
}

// MARK: - EmojiCell

private final class EmojiCell: UICollectionViewCell {
    let label = UILabel()

    override init(frame: CGRect) {
        super.init(frame: frame)
        label.font = .systemFont(ofSize: 28)
        label.textAlignment = .center
        label.adjustsFontSizeToFitWidth = false
        label.translatesAutoresizingMaskIntoConstraints = false
        contentView.addSubview(label)
        NSLayoutConstraint.activate([
            label.centerXAnchor.constraint(equalTo: contentView.centerXAnchor),
            label.centerYAnchor.constraint(equalTo: contentView.centerYAnchor),
        ])
    }
    required init?(coder: NSCoder) { fatalError() }

    override var isHighlighted: Bool {
        didSet {
            contentView.backgroundColor = isHighlighted
                ? UIColor.label.withAlphaComponent(0.12)
                : .clear
            contentView.layer.cornerRadius = 6
        }
    }
}
