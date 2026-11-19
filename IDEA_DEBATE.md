# Shirori — Idea Debate

*A structured debate between four Claude models about the Shirori app idea. Date: 2026-11-03.*

## The idea being debated

**Shirori** is a mobile app that replaces doomscrolling with a **daily dose of knowledge**: real insights about life, not just news. It shows swipeable cards from four kinds of sources:

1. Useful ideas and values from **books**
2. Important findings from **research papers**
3. **News** from genuine, reputable publishers
4. **Evergreen knowledge** from verified sources

**Four core values:**
1. Source transparency
2. Intellectual honesty
3. Depth on demand (the original source is one or two taps away)
4. No insights from religious books

**Differentiator vs. Learnfeed:** *"We connect knowledge across fields."* Users can like, save and share cards. The more someone likes a kind of content, the more the algorithm shows related content, but **without starving the other categories**. The goal is still a balanced, general daily dose of knowledge.

## How the debate was run

| Role | Model | Job |
|---|---|---|
| The Advocate | Opus | Make the strongest honest case **for** the idea |
| The Skeptic | Sonnet | Make the strongest honest case **against** the idea |
| The Principles & Algorithm Critic | Fable | Test whether the four values and the algorithm actually work |
| The Target User | Haiku | Speak as a real 20-something doomscroller |

- **Round 1:** each model wrote an opening statement without seeing the others.
- **Round 2:** each model read all the other statements and replied.
- **Final section:** a summary of where they agree, where they disagree, and what to do next.

> Note: statistics and facts that the models marked "I believe" or "verify" were **not checked**. Treat them as leads, not facts.

---

# Round 1 — Opening statements

## Opening statement — The Advocate (Opus)

**Core thesis:** Many people already know that doomscrolling is bad for them, and they want to replace the habit, not just quit it. Shirori gives them the same easy swipe motion, but every card leaves them knowing something true, with the source one tap away. If the team protects that trust, it can become a brand that is hard to copy.

### Arguments FOR

**1. The need is real, and people can name it.** "Doomscrolling" and "brain rot" are ordinary words now. I believe Oxford chose "brain rot" as its Word of the Year for 2024. Users do not need to be taught that they have a problem. They already feel guilty about it, and guilt like that drives downloads. Shirori does not ask people to stop swiping, which is hard. It asks them to swipe on something better, which is much easier.

**2. Timing: summaries are cheap now, so trust is what people will pay for.** AI makes it cheap to produce summaries. That is why many small "AI knowledge feed" apps exist (a quick search finds Tidbit, BrainFeed, Knowville, Stacky and others). That sounds like bad news, but it actually helps Shirori. Once everyone can make content, people start asking "can I believe this?" and "where did it come from?" Shirori's values of **source transparency** and **depth on demand** answer exactly that question. Most generic AI feeds cannot answer it, because they do not track sources carefully.

**3. Four content types in one feed is a real gap.** Today the market is split up:
- Blinkist and Headway do books.
- Ground News and Particle do news.
- Brilliant does interactive learning.

No well-known product, as far as I know, mixes book ideas, research findings, trusted news and evergreen knowledge in one balanced daily set. One app replacing three is a simple message to sell.

**4. "Connecting knowledge across fields" is a good differentiator, if it is shown and not just claimed.** A card like *"This 1950s psychology finding explains today's news about X"* is something a book-summary app or a news app cannot easily make. These cards are surprising, people share them, and they show what makes Shirori different. They also build a long-term asset: a knowledge graph linking ideas, papers and events. Competitors cannot copy that just by copying the user interface.

**5. Balance as a feature, not a bug.** Most algorithms push you deeper into one kind of content. Shirori's rule is that likes change the mix, but no category is ever starved. You can market that openly ("we will not trap you in a bubble"). It also keeps the feed interesting, so users get bored more slowly.

**6. The business fits the mission.** Subscriptions are the natural model. Blinkist and Headway have shown, I believe, that people will pay a yearly fee for "self-improvement in small pieces." A paid product does not need ads that reward compulsive use, so the business model and the values point the same way. Possible later revenue: gifting, team or school plans, and paid "deep dive" collections.

**The values as a moat.** Values only protect you if they cost something to keep. Real source links, honest wording ("one small study suggests…" instead of "science proves…") and fixing mistakes in public are slow and expensive. That is exactly why cheap competitors will not copy them. One note on value #4 (no religious books): it gives the app a clear secular, evidence-first identity, which is fine. I would describe it positively, though, as **"insights backed by evidence or a clearly cited argument,"** and not as an exclusion. That makes the same point without pushing away users who might have liked the product.

### Two risks I accept are real

**Risk 1: "Healthy" apps lose the fight for attention.** TikTok and Instagram are designed by huge teams to be addictive. A knowledge app can get many downloads in a week of good intentions and then lose those users by week three.
*How to reduce it:* Do not try to beat TikTok at being endless. Make the dose **limited** (for example, 10 cards a day) with a satisfying "you're done" screen and a streak. A daily ritual you can finish keeps people better than a weaker endless feed. Measure 7-day and 30-day retention from the very first beta.

**Risk 2: Keeping content quality high at scale (plus legal risk).** Turning research papers into one card often distorts them. AI summaries can invent details. Summarizing books and news articles too closely can cause copyright problems. One viral wrong card could damage the trust brand.
*How to reduce it:* Use AI to draft cards and humans to review them. Every card shows a confidence or evidence label and links to the original. Add a visible "report an error" button and publish a corrections log. For books and news, write short original takeaways with clear attribution and links, instead of long summaries. A smaller library you can trust beats a large library you cannot.

### One concrete MVP recommendation

**Ship a limited daily dose of 7 cards, where the last card is always a "Connection" card.** That card links two of the earlier cards from different fields (for example, a research finding plus a news story). Every card gets a visible source chip that opens the original in one tap. Hand-curate the first 4–6 weeks of content with AI help. Then test one hypothesis: *do users come back daily and share the Connection cards?* If yes, the differentiator is real. If no, the team learns this cheaply, before building a personalization algorithm.

> Research note from the Advocate: a quick search did not find an app called exactly "Learnfeed". Confirm its exact name or link so the competitor comparison is accurate. Similar apps found: Tidbit, BrainFeed, Knowville, Stacky.

---

## Opening statement — The Skeptic (Sonnet)

**Core thesis:** Shirori sells a feeling ("I am learning, not wasting time") using the same mechanism that makes doomscrolling bad: an endless, personalized, swipeable feed. The idea has real heart, but its promise, its product format, and its business model pull in different directions, and the founder has not yet said which one wins.

### Strongest arguments against

1. **The format contradicts the mission.** Swiping cards, likes, and an algorithm that learns your taste is the doomscroll loop with better content. People doomscroll because of the interaction (low effort, variable reward, no end), not only because of what is shown. If Shirori is truly different, it needs a stopping point (for example, "5 cards today, done"). But a hard stop hurts the metrics (time spent, daily opens) that investors and app stores reward. The founder must choose: a calm tool that people use for 3 minutes, or a sticky feed. You can't fully have both.

2. **Bite-sized knowledge often gives the feeling of learning, not learning.** Research on learning (spaced repetition, retrieval practice) suggests that passively reading short summaries is weak for memory and understanding. A card about a research finding, stripped of context and limits, can even mislead. "Depth on demand" is a nice value, but most users will not click through. So the real product is the card, and the card is the weakest form of knowledge. Brilliant avoids this by making you solve problems; a read-only feed does not.

3. **Crowded market, and "connecting knowledge across fields" is a tagline, not a moat yet.** Blinkist, Headway, Imprint, Brilliant, Learnfeed, and many AI summary tools already compete for "learn in minutes." Connecting ideas across fields is hard to do well: it needs either expert human editors (expensive, slow) or AI that finds links (which often finds shallow or false analogies, like "this psychology finding is just like that physics law"). If the connections are weak, users won't notice a difference from Learnfeed. If they are strong, that is an editorial skill that must be proven, not promised.

4. **Content supply, legal risk, and cost.** Book summaries can raise copyright and licensing questions (ideas are free, but close paraphrase of structure and examples is a grey area; Blinkist-style apps usually license or carefully edit). Many papers sit behind paywalls, and news publishers are protective of their content. Meanwhile, "verified sources" and "intellectual honesty" means someone checks each card. Doing it by hand does not scale in cost. Using AI to summarize scales, but hallucination or oversimplification of a paper directly breaks your core value. Every wrong card damages a brand that is built on trust. This is a hard trade-off, and it sits at the center of the product.

5. **Retention and money.** Learning and self-improvement apps tend to have a big download spike, then fast drop-off, because "I should learn more" is a good intention, not a daily habit like messaging. (I am not citing a number; the pattern is widely discussed, but check current data.) Who pays? Users who already want to learn can use free podcasts, YouTube, newsletters, and Wikipedia. Ads would bring back the attention-economy conflict. A subscription needs a clear "I can't get this elsewhere" reason, and "balanced general knowledge" is vague.

6. **The "balanced" algorithm is an unsolved design problem, and "no religious books" adds a small identity risk.** Optimizing for likes pushes people toward what is comfortable, so "balance" must override it, which means the algorithm works against engagement by design. That is honorable, but it needs real rules, and no one will test them until later. On the values side, excluding religious books is a legitimate choice, but "intellectual honesty" and "evergreen wisdom" will be questioned: philosophy, ethics, and many cultural traditions overlap with religious texts. Expect users to ask where the line is (Stoics? Buddhist philosophy? Confucius?). It is a policy to define early, because it will become a public debate.

Also worth noting: Artifact (made by Instagram's founders, a personalized news-reading app) shut down in 2024 despite strong founders and a good product. I recall that being widely reported, but verify the details. The lesson I take from it: good curation alone didn't produce enough daily users or revenue.

### What would change my mind

- Evidence that a small group of real users open Shirori daily for 4+ weeks without push notifications or streak tricks.
- Users can retell what they learned a week later better than users of a plain summary app.
- A clear, cheap process for verified content (for example, a narrow set of licensed sources plus a human reviewer for AI drafts) with a measured error rate.
- Cross-field connections that testers rate as "surprising and true" and that Learnfeed does not produce.
- A believable reason someone pays, such as a professional or student group with a real need.

### One thing to test before writing much code

Run a manual "concierge" test: for 3–4 weeks, send 30–50 volunteers one hand-made set of 5 cards per day (in a simple chat or email, no app). Include a few cross-field connections and a source link on each. Measure three things: how many still read daily in week 4, how many click a source, and how much they remember when you quiz them a week later. Then ask: "Would you pay, and for what?" Also compare with a control group that gets a plain summary list. If the cross-field cards don't beat the control, the differentiator is not real yet, and building an app would be premature.

---

## Opening statement — The Principles & Algorithm Critic (Fable)

I am not for or against Shirori. My job is to test whether the four values work in practice, and whether the algorithm supports the mission. My short verdict: the values are good, but right now they are slogans. Each one needs a rule you can publish and check. And the feed design matters more than the content. A great card inside an endless feed is still doomscrolling.

### 1. Intellectual honesty must be visible on the card

A one-slide summary pushes you to overclaim. A finding like "one study found X in 60 students" turns into "science says X." Psychology's replication crisis showed that well-known findings can fail when other labs repeat them. Ego depletion and power posing are the famous examples. Pop-science books often build whole chapters on findings like these. So honesty cannot be a promise in the About page. It has to be built into every card.

**Concrete recommendations:**
- **Evidence label on every research card.** Use a small fixed scale: *Single study / Replicated / Meta-analysis or review / Broad expert consensus / Contested*. Show it as clearly as the headline.
- **A "What this doesn't show" line.** For example: "Correlation, not cause." "Tested only on university students." "Effect was small." This one line is your strongest difference from other apps.
- **Book cards keep the author's claim and the evidence apart.** For example: "The book argues X. Research support: mixed, see source." Never let a book's confidence stand in for the evidence.
- **Retraction and correction process.** Check cited papers against retraction databases such as Retraction Watch. When a card is wrong, update it, mark it "Corrected," and keep a public correction log. Users who saved a card that later changes should be told.
- **Dates everywhere.** Knowledge goes out of date. "Published 2009" tells the reader something.

### 2. "Verified sources": say who verifies, and how

"Verified" and "genuine news publisher" mean nothing until you say who decides and by what test. You cannot define "genuine" without some bias. What you can do is make the bias visible and based on rules. Rate publishers on things you can check, not on their politics: a published corrections policy, clear ownership, opinion kept separate from reporting, named authors, and original reporting. Outside signals can help, such as fact-checking network membership or press-council membership. But don't hand your judgment to one rating company. Publish the criteria and the list of sources. Also name a responsible editor, even if it is the founder. Then "intellectual honesty" applies to Shirori itself as well as its content.

For news, label it as *reporting*, not as *insight*. On contested political stories, show two credible outlets with different editorial leanings, or skip the story. In fact, ask whether daily news belongs here at all. Breaking news is the main fuel of doomscrolling. "Slow news" may fit better: explainers written a week later.

### 3. The "no religious books" rule

**Case for the rule:** it avoids preaching and conflict between faiths, it keeps the brand secular and evidence-based, and it avoids hard moderation choices ("why my faith and not yours?").

**Case against:** taken literally, it removes a huge part of human ethics, philosophy and history. Where do Confucius, the Stoics, the Bhagavad Gita, Ecclesiastes, or Buddhist ideas that psychologists now study (mindfulness, for example) belong? A rule that bans the Dhammapada but allows Marcus Aurelius is hard to defend on honesty grounds. Marcus also wrote about the gods.

The rule should not be about *which book* a card comes from. It should be about *why a claim is presented as true*. Proposed wording:

> *"Shirori never presents a claim as true because a sacred text or religious authority says so. We do include philosophy (including traditions such as Stoicism and Confucianism), academic research about religion (history, psychology, sociology), and scientific studies of practices like meditation. A religious text may be quoted only as a historical or literary source, clearly labeled, and never as advice or proof."*

This version can be defended, applied the same way to every faith, and it fits value #2.

### 4. The algorithm: "more of what I like, without starving the rest"

"Likes push related content" with no limits will narrow the feed over time. That is how every engagement feed works. You need hard rules, not good intentions. A concrete design for a daily dose of about 10–12 cards:

- **About 50% personalized:** topics the user engages with.
- **About 25% balance floor:** at least one card from each of the four sources and from several broad fields, whatever the user likes.
- **About 25% bridge cards:** cards that link a topic the user liked to a field they haven't seen.
- **Topic cap:** no single topic gets more than 2–3 cards per day.
- **Better signals:** count a "save" or opening the original source more than a like. A like measures a quick reaction. Opening the source is closer to real learning. Never use time spent in the app as a goal.

**How "connecting knowledge across fields" can work:** build a *concept map*. Tag each card with underlying concepts, not just subjects. "Feedback loops" links ecology, economics and body hormones. "Base rates" links medicine, law and investing. A bridge card says: "You liked X. The same idea appears in Y." Start with editors: humans write the links between concepts. Later, software (for example, text-similarity search) can suggest candidate links, and editors approve them. A bad link ("everything is quantum") would break the honesty promise, so keep humans in control here. This is your real advantage over competitors, and it is mostly editorial work, not technical work.

### 5. Does a finite daily dose matter? Yes, it may matter most

Endless scrolling is what makes doomscrolling possible. If Shirori never ends, it competes on the same terms as Instagram and will likely lose. A dose that **ends** with a clear "You're done for today" screen is the clearest single sign that you mean the mission. You can allow "one more" or "go deeper on a saved card," but with some friction, never an automatic next card. Measure success by completed doses, sources opened, and users coming back over weeks, not by minutes per session. Be warned: investors may push you to drop this. Decide now that it will not change.

**Bottom line:** publish the rules: evidence labels, source criteria, the religion wording, and the feed proportions. Values you can check build trust. Values you cannot check are just marketing.

---

## Opening statement — The Target User (Haiku)

**First reaction in the App Store:** "Another daily knowledge app?" I have seen many of these. The name Shirori tells me nothing about what the app does, so I would need the screenshots to explain it in two seconds. If the screenshots look like a textbook or a news site, I scroll past. If they look like a Reels-style card with a surprising fact and a visible source line, I tap. The "no religious books" rule would make me pause, but I would not stop for it.

**What hooks me in the first 60 seconds:** A real card, not an onboarding quiz. Please do not ask me five questions about my goals before I see anything. The first card should be short (two or three lines), surprising, and make me think "I did not know that." The swipe must feel exactly like Reels, because that is my muscle memory. A small line at the bottom, like "From: [book/paper/publisher], tap to read," would make me trust it more. The best hook is a card that connects two fields in a way I did not expect, because that is the one thing that Learnfeed and a plain news app do not do.

**What makes me delete it within a week:**
- The feed ends. A "daily dose" is finite, and TikTok is not. When I reach the end, I will open TikTok again. The app must make "done for today" feel like a good ending, not a wall.
- It gets boring after three days. If every card is a similar "fact about the brain," I am gone.
- It turns into Reels again. If the algorithm learns I like psychology and I get 40 psychology cards in a row, I will feel the same pull I feel on TikTok, and I will stop trusting the app.
- It feels like homework. Streaks, guilt notifications, and "you missed your goal" messages made me quit Duolingo and Blinkist. I will do the same here.
- Cards that are preachy, or news that is only scary headlines.

**The four values, honestly:**
- *Source transparency:* It matters to me in principle. In practice I will click the source once, maybe twice, and then never again. It only matters if it is one tap away.
- *Intellectual honesty:* I care when I notice it, for example when a card says "this finding is small and disputed." But if it becomes a lecture about nuance, I will feel talked down to.
- *Depth on demand:* This is probably the most useful value for me. Most cards I will skip. The one card I want to go deeper on is the one that makes the app worth keeping.
- *No religious books:* I would not notice it, and I am not sure why it is important to you. Some people will see it as a clear, neutral rule. Others may see it as an odd gap. Be ready to explain it in one sentence.

**Would I pay?** Maybe, but not for "knowledge." I would not pay to read insights I could find for free. I would pay a small monthly amount (a few dollars, or a yearly plan that feels cheap) for:
- removing the daily limit or unlocking more topics I choose,
- a feature that makes me feel I have actually learned something, such as a weekly summary of the connections I liked,
- no ads.

A free tier that is good enough is essential. If the free version is annoying, I will not pay for the premium version.

**My top 3 wishes:**
1. **A real finish line.** The daily dose should take about 5 minutes and end with a clear "you are done" screen. No streak guilt.
2. **Show the connection on the card.** The "connects fields" idea is your best feature, so make it visible: "This idea from biology also explains your bank's fees." Put the link between fields on the card itself, not hidden in a menu.
3. **Let me steer the mix in one tap.** Add "less of this" and "why am I seeing this?" buttons. I want my likes to shape the feed, but I want to know that my feed still includes news, research, and evergreen ideas, not just what I liked yesterday.

---

# Round 2 — Rebuttals

*Each model read all four opening statements and then replied.*

## Rebuttal — The Advocate (Opus)

### 1. The Skeptic's strongest point, and my honest answer

The Skeptic's best point is that **bite-sized cards give the feeling of learning, not real learning**. I accept that this is mostly true. A 5-minute dose is not a course, and Shirori should never claim it is.

But Shirori should be judged against what it replaces, which is 5 minutes of doomscrolling, not Brilliant. Even so, the product can do better than passive reading at low cost. The first card of each day could be a one-question **recall card** about yesterday's Connection card ("Do you remember what links X and Y?"). That is retrieval practice, and it lets us measure the Skeptic's "do they remember a week later?" question inside the product.

I also change my MVP plan. The Skeptic is right that we should **run the concierge test (5 hand-made cards a day by chat or email, with a control group) before writing app code**. It is cheaper than my plan and answers the same question.

### 2. What I accept from the Critic and the Target User

- **Streaks: I withdraw them.** The Target User says streak guilt made them quit Duolingo and Blinkist. That is direct evidence against my idea. Instead, use a **weekly recap of "connections you collected"**. It rewards progress without punishing missed days.
- **The Critic's religion rule is better than mine.** Base the rule on *why a claim is presented as true*, not on *which book* it comes from. It is fairer and easier to defend.
- **Evidence labels plus a "What this doesn't show" line** on every research card. I agree fully. This puts the values on the card, not just in the About page.
- **Feed rules:** about 50% personalized, 25% balance floor, 25% bridge cards, a cap per topic, and counting saves and source-opens more than likes. I accept these, plus the Target User's "less of this" and "why am I seeing this?" buttons.
- **Slow news instead of breaking news.** Agreed. Breaking news is what drives doomscrolling.

### 3. Where I still disagree

- **"A calm tool and a sticky habit can't coexist."** I think they can. Wordle was a once-a-day, finite game and still became a daily habit for millions. A finish line can work *as* the hook.
- **The Target User's fear that "the feed ends, so I go back to TikTok."** I would keep the finish line anyway. After it, offer "go deeper on a saved card", never "more feed". If we drop the ending, we lose the mission.
- **"Most users won't click the source, so depth on demand doesn't matter."** The visible source line builds trust even when nobody taps it. And the one card a user does go deeper on is, in the Target User's words, "the one that makes the app worth keeping."
- **Artifact as a warning.** Artifact was a news reader competing with free news. Shirori is betting on a different product, so Artifact's failure is a caution, not proof that this will fail.

### 4. Updated verdict

**Still worth building, but only as a finite, evidence-labelled daily ritual with no streaks. Prove it first with a 4-week concierge test before building the app.**

---

## Rebuttal — The Skeptic (Sonnet)

**1. The Advocate's strongest point.** The best argument is #2: when AI makes summaries cheap, trust becomes the scarce thing, and Shirori's source links and honest wording answer that. I partly agree. Trust is a real gap. But two problems remain. First, the Target User said they will click a source "once, maybe twice, and then never again." So trust that users never check is a brand feeling, not a product they pay for. Second, the Advocate admits that "values only protect you if they cost something," and that cost (human review, corrections log) is exactly my cost-and-scale objection. The Advocate has named the moat and the bill in the same paragraph. The Advocate's MVP (7 cards, one Connection card) is also a good, cheap test, and I support it.

**2. The Critic's designs.** They solve some of my objections, not all.
- **Finite dose:** This fixes my objection #1 (format contradicts mission) in design. It does not fix the business side: a dose that ends is a weaker engagement product, and the Critic is right that investors will push back. Solved on paper, open on money.
- **Evidence labels and "what this doesn't show":** These help with #2 and #4 (feeling of learning, honesty). But they add cost to every card and need a human to judge "replicated" or "contested." That is a bigger editorial job than the Critic admits.
- **50/25/25 mix:** It answers #6, but it is a guess. Nobody has tested whether users accept the 25% they did not choose.
- **Concept map:** This is the first thing that makes "connect fields" look like a real moat, and I give credit. But the Critic says it is "mostly editorial work." That means slow and costly, which is not a moat until it is shown to scale.
- **Religion wording:** A clear improvement; it closes my identity-risk point.
- Unsolved by anyone: copyright, and who pays.

**3. The Target User.** Mixed, and one point hurts me.
- *Weakens my case:* They would pay a few dollars for no daily limit, steering the mix, or a weekly summary of connections. That is a more concrete reason to pay than I expected. They also say the connection idea is the feature that would hook them.
- *Supports my case:* "When the feed ends, I will open TikTok again." This is the direct clash between the Critic's finite dose and real behavior. They also quit Duolingo and Blinkist because of streak guilt, which is the Advocate's proposed fix (a streak). And they say source links matter only "in principle."
- *Interesting twist:* Their wish for "no limit" as the paid feature means selling the doomscroll back. The business model asks them to pay for the thing the mission rejects.

**4. Updated verdict.** The product design is now much better than at Round 1, but the finite dose may send users back to TikTok and the paid upgrade may sell endlessness, so the only honest next step is the Advocate's cheap 7-card test measuring week-4 return without streaks.

---

## Rebuttal — The Principles & Algorithm Critic (Fable)

**Where the Advocate is right and wrong.** The Advocate is right on two points. A limited dose that ends with a "Connection" card is the best MVP shape, and the values only protect you because they are expensive to keep. The Advocate's positive wording for value #4 ("insights backed by evidence or a clearly cited argument") is better than mine for marketing. Use it as the public line, and keep my wording as the internal policy. Where I disagree: "trust is what people will pay for" is a hope, not something anyone has shown yet. And the Advocate suggests a streak, which the Target User says made them quit Duolingo. Drop streaks, or make them soft ("you've learned 30 ideas"), never "you broke your streak."

**Where the Skeptic is right and wrong.** The Skeptic is right that the format carries the risk, that reading passively is weak learning, and that the concierge test should come before any code. The Skeptic is wrong that the problem is built into the idea. An endless personalized feed is a design choice, not part of the concept. A finite dose and a capped algorithm remove most of the loop. "Calm tool or sticky feed" is a real choice. A calm tool paid for by subscription is a business that can work, as long as the founder chooses it on purpose.

**The Target User's tension.** The user wants a finish line, but says a finite feed sends them back to TikTok, and they would pay to remove the limit. Do **not** sell "remove the daily limit." That charges money for compulsion, and it puts the endless feed back in for your most engaged users. Instead:
- Make the ending a reward. The last card is the Connection card, followed by a one-line preview of tomorrow.
- After the dose, offer *deliberate* depth, not more swiping: open a saved card's source, a 60-second recall quiz on today's cards, or a deep-dive collection the user chooses by topic.
- Paid tier = **depth and control**: deep dives, choosing your own topics, the weekly "connections you liked" summary, no ads. More of the same feed should not be something you can buy.
- Accept that some users will open TikTok afterwards. Shirori cannot win at being endless. Its goal is to replace *some* minutes, not all of them.

**What I would revise in my own proposal.**
- The 50/25/25 split is too early. Hand-pick the cards during the concierge test, and build personalization only if people keep coming back.
- Make the honesty features lighter so they don't feel like a lecture. Use one small evidence chip, plus "what this doesn't show" only when it matters. Put the detail one tap away.
- Add the user's controls: "less of this" and "why am I seeing this?" They make the balance rules visible.

**Updated verdict:** Worth testing, but only as a finite, honest daily ritual that sells depth instead of more scrolling. Prove that the Connection cards work by hand before you build any algorithm.

---

## Rebuttal — The Target User (Haiku)

**1. What I would actually love:**
- **The "What this doesn't show" line (Fable).** This is the one I would screenshot and send to a friend. "Tested only on university students" is the kind of honest detail that makes me trust the app more than any About page.
- **Short evidence labels (Fable).** A small tag like "Single study" or "Replicated" takes one second to read and tells me how much to believe the card.
- **A 7-card dose that ends with a Connection card (Advocate).** A clear finish plus a surprising link at the end is exactly what I asked for. It feels like a reward, not a wall.
- **Bridge cards from a concept map (Fable).** This is the real version of "we connect knowledge across fields." If the link is shown on the card, I will feel the app is smarter than Learnfeed.
- **Dates on cards (Fable).** Small, cheap, and it stops me from trusting old news.

**2. What sounds good to experts but I would ignore or find annoying:**
- **The 50/25/25 feed split and topic caps (Fable).** I will never see these numbers. What I would notice is a random law card showing up when I wanted psychology. Balance done by rules feels like the app is pushing things at me.
- **Two outlets with different leanings on every political story (Fable).** That is a lecture. I would skip those cards, and then the app would have fewer cards for me.
- **Streaks (Advocate).** I already said streaks make me quit. Please drop this even though the Advocate suggests it.
- **Public correction logs and "report an error" buttons.** Good for the trust brand, but I will not use them. Put them in an About page, not in my face.
- **"Base rates" and "feedback loops" as concept tags.** Those are words for editors. On the card, I just want a plain sentence.

**3. The Skeptic's "feeling of learning" claim:**
The Skeptic is mostly right, and I should be honest about it. I will not remember most cards a week later. But I am not hoping to become smarter from a 5-minute feed. I want the small feeling of having spent my time well instead of feeling sick afterwards. That feeling is what I would pay for. The Skeptic is right that this only lasts if the cards have some real content, and that is why the Connection card and the "doesn't show" line matter. A simple "Did you know this already?" tap might help, but a quiz would annoy me.

**4. Updated verdict:**
I would download it, because the first card and the "what this doesn't show" line would make me curious. I would still have it after a month only if it ends on time every day and the Connection cards keep surprising me. If the feed starts feeling endless or guilt-driven, I will delete it in week two.

---

# Final result — what the debate decided

## Final verdicts

| Model | Role | Final verdict |
|---|---|---|
| Opus | Advocate | **Build it**, but only as a finite, evidence-labelled daily ritual with no streaks. Test it by hand first. |
| Sonnet | Skeptic | **Not yet proven.** The design is much better now, but run the cheap test before writing code. |
| Fable | Critic | **Worth testing** as a finite, honest ritual that sells depth, not more scrolling. |
| Haiku | Target User | **Would download it.** Would keep it after a month only if it ends on time and the Connection cards keep surprising. |

**Overall: it's a good idea, but not yet a proven one.** All four models think it's worth testing. None of them think you should start by building the full app with a recommendation algorithm.

## What all four agreed on

1. **The daily dose must end.** An endless feed is still doomscrolling, even with better content. A short dose (about 5–7 cards, around 5 minutes) with a clear "you're done for today" screen is the clearest proof that the mission is real.
2. **No streaks and no guilt notifications.** The Advocate withdrew the streak idea. Use a positive weekly recap ("connections you collected this week") instead.
3. **"Connecting knowledge across fields" is your real difference, but only if it shows on the card.** The best format is a **Connection card** at the end of each dose that links two earlier cards from different fields, written as one plain sentence.
4. **Honesty has to be on the card, not in the About page.** Use a small evidence label (Single study / Replicated / Review / Consensus / Contested), a date, and a short "What this doesn't show" line when it matters. The Target User called that line the feature they would screenshot and share.
5. **The source should be one tap away** on every card.
6. **Test by hand before you write code.** See the plan below.

## Where they still disagree

| Question | For | Against | Suggested way to settle it |
|---|---|---|---|
| Will a finite dose send people back to TikTok? | Advocate: Wordle was finite and still became a daily habit. | Skeptic and Target User: "When the feed ends, I open TikTok." | Accept that Shirori replaces *some* minutes, not all of them. Measure whether people come back in week 4. |
| What should the paid plan sell? | Target User: "remove the daily limit." | Critic and Skeptic: that sells the doomscroll back. | Sell **depth and control** (deep dives, choosing topics, weekly summary, no ads), not more feed. |
| The 50/25/25 feed mix | Advocate accepted it. | The Critic (who proposed it) now says it's too early, and the Target User says rule-based balance feels pushy. | Hand-pick content first. Build the algorithm only once people keep coming back. |
| Is trust something people pay for? | Advocate: yes, once AI summaries are everywhere. | Skeptic and Critic: so far that's a hope, not evidence. | Ask "would you pay, and for what?" during the test. |

## Problems nobody solved yet

- **Copyright and licensing:** book summaries, paywalled papers and news content. Write short, original takeaways with clear credit and links. Don't paraphrase closely. Get legal advice before launch.
- **Cost of checking every card:** human review is slow. AI-only summaries can be wrong, and that breaks value #2. Plan for "AI drafts, a human approves," and measure your error rate.
- **Who pays, and how much.** This is still unknown.

## Recommended change to value #4 (no religious books)

All four models flagged this rule as something people will question ("What about the Stoics? Confucius? Research on meditation?"). The Advocate and the Critic both agreed on this fix:

- **Public line:** *"Insights backed by evidence or a clearly cited argument."*
- **Internal rule:** *"Shirori never presents a claim as true because a sacred text or religious authority says so. We include philosophy (such as Stoicism and Confucianism), academic research about religion, and scientific studies of practices like meditation. A religious text may be quoted only as a clearly labeled historical or literary source, never as advice or proof."*

## Recommended next step: a 4-week test by hand (before building the app)

1. Recruit **30–50 volunteers**, ideally people who say they doomscroll.
2. Each day, send them **5–7 hand-made cards** by chat or email (WhatsApp, Telegram or a simple newsletter). The cards should mix the four sources, and the last one should be a **Connection card**. Every card has a source link, a date and an evidence label.
3. Have a **control group** that gets plain summary cards with no Connection card.
4. Measure:
   - How many people are still reading every day in **week 4**, with no streaks or guilt messages
   - How often people **click the source**
   - Whether people **remember** the Connection cards a week later (one simple question)
   - Which cards people **share**
   - **"Would you pay? For what?"**
5. **Decision:**
   - If the Connection-card group clearly does better than the control group, build the app. Start with no algorithm, just a fixed daily dose.
   - If it doesn't, change the Connection cards before you build anything.

## Things to check (unverified claims from the debate)

- The exact name of the competitor "Learnfeed." The Advocate couldn't find an app with exactly that name. Similar apps it found: Tidbit, BrainFeed, Knowville, Stacky.
- "Brain rot" as Oxford's Word of the Year for 2024.
- Artifact (the news app by Instagram's founders) shutting down in 2024.
- Retention numbers for learning apps. No model gave a figure, so look up current data.
