# #18 · 偏好学习作者验证（进行中）

Own worktree `/tmp/wardrobe-ticket-18`, branch `codex/ticket-18`, accepted source18b7b191; latest docs integration1bd7737 included. #17 CLOSED prerequisite verified by root, comment6026218600. Independent acceptance pending. No push/issue close.

Public seams: shared Application persisted SQLite, same-source actual CLI and stdin MCP with temporary limited authorization, GUI through that CLI. Only synthetic temporary inventory/actualwear fixtures; no user data/keys/model purchase.

## Completed vertical evidence

- explicit-red.log: learning.set unsupported; explicit-green.log durable source/version/null inferred/idempotency/reopen and0wear. explicit-compile-error.log preserves initial Rust compile errors, not behavior red.
- recompute-red.log → recompute-green.log: known like+overall5+actualwear source snapshots yieldsblue5; repeatedsource weights identical,wearfacts unchanged.
- ranking-red.log → ranking-green.log: learned/explicitblueweight changes sharedranking; fixedred/excludedblue hard constraints prevail.
- management-red.log → management-green.log: appendeditCAS, delete suppression survivesordinaryrecompute, reset cutoff preventsoldfactsreappearing; include_prior=true explicitly rebuilds them, preservingactualwear andexplicitprefs.
- history-red.log → history-green.log: garmentcurrentblue→red edit doesnot rewritebluewearsnapshot; effectivewearrevocation removes learned signal; paginatedhistory reads distinctversions.
- score-red.log → score-green.log: skipalone contributesnothing, comfort5cotton+2 vsstyle1blue−2, optionaloverall actsallfeatures. Algorithm definedADR0025, not predictiveaccuracy.
- duplicate-red.log → duplicate-green.log: conflicting explicitroots foronefeature must useedit; twoindependentconnections sameexpectedrevision exactlyonewins.
- public-contracts.log: member isolation, limitedoperation/member authorization, invaliddimensions/weights andDB20 actualmissingtablesrepair.
- gui-red.log: visible memoryentrymissing; gui-red-cleanup-timeout.log preservesfirstmaskedcleanup timeout. gui-closed-details-fixture-error.log tried hiddencheckbox withoutopeningdetails; fixedtestclicksactualsummary. gui-readable.log:2 actualGUIcases passed: explicitsave/edit/reload/delete/reset, realexistingwearsource/view/correction→ranking, forget/recompute/fullrebuild, factspreserved, sevenwidths. Source/event screenshots320/1280 reviewed.
- actual-entry.json + reproduce-entry.py: actuallimitedCLI/stdinMCP all7learningops/recommendation/wearread,8paginatedmutations,oneexistingwearpreserved. Initialfixtureexpected9afterperforming8, entry-fixture-count-error.log retains correction; notdomainfailure.

## Scope and remaining checks

Explicit-user preferences override same-featureinference; unlikebody/clothingfacts, they areeditableweights. Algorithm explicit-and-events-v1 counts eachfeatureonceperevent:like+2/dislike−2, otherreaction0, actualeffectivewear+1; overallN−3allfeatures, comfortmaterial, stylecolor/pattern/style; missing0, weightclamped−5..5. Effectivecurrentfeedback/wear revision IDs areprovenance; original snapshotsused. Sourcecreated/updatedtime isnotactualweardate. ResetcutoffisUTCeventrecordtime. Delete/revokehistory retainedforaudit whileinactiveeffectgone. Ordinaryrecompute retains suppression/cutoff; explicitfullhistoryrebuild clears them. No fact/size mutations or implicitwear.

DB20 actualtables learning_memories/profiles/state/history/receipts andindexes, unconditionalmissingtablecreation/maxversion. Future transport/backup consumingrootmustextendexplicitbusinessallowlist, including suppression/reset/provenance/history; coordinatedwithroot/27/32, no catchallDB dump. No platform35/fullBeta/paidcloud/nativeTauri/otherOS completion claimed.

Whole workspace/clippy/same-source finalCLI/MCP, realtemporaryOS crypto, fullGUI and10k/100kwithnonemptylearning remain underway; terminaltotals added only after ownerpoll.


## Checkpoint boundary

See async-next.md. Current source remains a checkpoint: synchronous recompute longtask must be replaced by durable asynchronous enqueue/worker before final candidate. Author terminal workspace188/12ignored;OS9;firstfullGUI51/5skipped;learning-scale1passed(65explicit/3inferred/100kactualsources,5587msrecompute/5111msrank);actualCLI/MCP pass. Confirmed cached-old-intent UIred79954→90432 scoped3GUIgreen, preserving503samekey retries. FullGUI predates this UIcallback change and is not claimed final-source completeUI. Separate initial TScallback propcompileerror retained. No acceptance, push or issueclose.
