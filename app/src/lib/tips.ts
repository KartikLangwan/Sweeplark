// Short lines shown while something takes time: app tips, and now and then a word about
// the developer. English first, Hinglish second.

export const TIPS: [string, string][] = [
  ["Tip: close Sweeplark's window when you're done. It keeps watching from the tray using almost no RAM.", "Tip: kaam ho jaye toh window band kar dein. Tray se Sweeplark lagbhag bina RAM ke nazar rakhta hai."],
  ["Tip: Boost never touches the app you were using, voice chat or anti-cheat.", "Tip: Boost aapki chalti app, voice chat aur anti-cheat ko kabhi nahi chhoota."],
  ["Tip: every Optimizer change can be switched off again, and History can undo it.", "Tip: Optimizer ka har badlav wapas band ho sakta hai, aur History se undo bhi."],
  ["Tip: turn on Game mode in Settings: background apps calm down when a game is full screen.", "Tip: Settings mein Game mode chalu karein: game full screen par background apps shaant ho jaati hain."],
  ["Tip: RAM guard frees RAM by itself when memory stays nearly full (Settings > Boost).", "Tip: RAM guard RAM bharne par khud khaali karta hai (Settings > Boost)."],
  ["Tip: files that look like yours (documents, photos) are never deleted without your tick, and go to the Recycle Bin.", "Tip: aapki jaisi files (documents, photos) bina tick ke kabhi delete nahi hoti, aur Recycle Bin mein jaati hain."],
  ["Tip: Storage > Duplicates finds files you have more than once.", "Tip: Storage > Duplicates mein ek hi file ki kai copies milti hain."],
  ["Tip: the Apps page removes programs and shows which ones have updates.", "Tip: Apps page se programs hatayein aur unke updates dekhein."],
  ["Tip: ask the Assistant \"why is my PC slow?\" It reads your PC's real numbers, offline.", "Tip: Assistant se poochiye \"mera PC slow kyun hai?\" Woh offline asli numbers dekhta hai."],
  ["Tip: right-click the tray icon for Boost, Clean and more without opening the window.", "Tip: tray icon par right-click: Boost, Safai aur bahut kuch bina window khole."],
  ["Tip: a restart once a week clears memory leaks that no cleaner can.", "Tip: hafte mein ek restart woh memory leaks saaf karta hai jo koi cleaner nahi kar sakta."],
  ["Tip: Startup also lists scheduled tasks that start updaters at sign-in.", "Tip: Startup mein woh scheduled tasks bhi dikhte hain jo sign-in par updaters chalate hain."],
];

export const PROMOS: [string, string][] = [
  ["Sweeplark is made by Kartik Langwan. See more of his work at kartiklangwan.runs-on.dev", "Sweeplark Kartik Langwan ne banaya hai. Unka aur kaam: kartiklangwan.runs-on.dev"],
  ["Like Sweeplark? A star on GitHub helps a lot: github.com/KartikLangwan/sweeplark", "Sweeplark pasand aaya? GitHub par ek star bahut madad karta hai: github.com/KartikLangwan/sweeplark"],
  ["Free, open source, no ads, no tracking. Built by Kartik Langwan.", "Free, open source, na ads, na tracking. Kartik Langwan ki banayi."],
];

/** The line to show at step `n` (every fourth one is a promo). */
export function tipAt(n: number, hinglish: boolean): string {
  const pick = n % 4 === 3 ? PROMOS[Math.floor(n / 4) % PROMOS.length] : TIPS[(n - Math.floor(n / 4)) % TIPS.length];
  return hinglish ? pick[1] : pick[0];
}
