-- note_annotations_dump.lua
--
-- Ground-truth extractor for the rssp `note_annotations_parity` test.
--
-- This is the *reference* side of the crossover-cue parity check. It runs
-- inside the real ITGmania engine (specifically the 8ms fork the Crossover
-- Cues feature targets) and turns `Steps:GetNoteAnnotations()` into the exact
-- JSON shape the Rust parity test expects.
--
-- It is designed to be dropped into pnn64/itgmania-reference-harness next to
-- the code that already emits per-chart baselines (tech_counts, hashes, etc.):
-- call `NoteAnnotationsForSteps(steps)` and attach the result to each chart
-- record under the key `note_annotations`, keeping the harness's existing
-- md5/baseline-path layout so rssp's lookup (baseline/<md5[0:2]>/<md5>.json.zst)
-- keeps working unchanged.
--
-- Field semantics (verified against Simply-Love-SM5-8ms/.../CrossoverCues.lua):
--   annotation.beat           -> beat (float)
--   annotation.footPlacement  -> table { column(1-indexed) = foot }; the number
--                                of keys is the foot count for that row.
--   annotation.tech           -> list of TechCountsCategory enums; a row is a
--                                crossover iff one short-string == "Crossovers".
--
-- The dump converts columns to 0-indexed (to match rssp's `column_mask` bit
-- positions), emits `feet` parallel to `columns` (the `footPlacement` foot id
-- per column), and tallies the per-row `tech` list into a `tech_counts` object
-- (one count per TechCountsCategory). Together these mirror rssp's full
-- `RowAnnotation` (feet + per-row tech); crossover-ness is derived from
-- `tech_counts.crossovers > 0`, not emitted as a separate field.
--
-- IMPORTANT — version pinning: StepParity cost weights determine foot
-- placement, and foot placement determines crossover classification. Always
-- regenerate baselines from the exact engine build the feature targets (record
-- the ITGmania commit alongside the baselines). Stock ITGmania != the 8ms fork.

local function note_annotations_for_steps(steps)
	local out = {}
	if steps == nil then
		return out
	end

	local ok, annotations = pcall(function()
		return steps:GetNoteAnnotations()
	end)
	if not ok or annotations == nil then
		return out
	end

	-- Map TechCountsCategory short-strings to the tech_counts field names.
	local tech_key = {
		Crossovers = "crossovers",
		HalfCrossovers = "half_crossovers",
		FullCrossovers = "full_crossovers",
		Footswitches = "footswitches",
		UpFootswitches = "up_footswitches",
		DownFootswitches = "down_footswitches",
		Sideswitches = "sideswitches",
		Jacks = "jacks",
		Brackets = "brackets",
		Doublesteps = "doublesteps",
	}

	for annotation in ivalues(annotations) do
		-- (column, foot) pairs sorted by column (0-indexed) so the columns and
		-- feet arrays stay aligned. foot is the StepParity foot id
		-- (None=0, LeftHeel=1, LeftToe=2, RightHeel=3, RightToe=4).
		local cf = {}
		for col, foot in pairs(annotation.footPlacement) do
			cf[#cf + 1] = { col - 1, foot }
		end
		table.sort(cf, function(a, b) return a[1] < b[1] end)
		local columns, feet = {}, {}
		for _, p in ipairs(cf) do
			columns[#columns + 1] = p[1]
			feet[#feet + 1] = p[2]
		end
		local note_count = #columns

		-- Full per-row tech: count each TechCountsCategory the row triggers.
		local tech_counts = {
			crossovers = 0, half_crossovers = 0, full_crossovers = 0,
			footswitches = 0, up_footswitches = 0, down_footswitches = 0,
			sideswitches = 0, jacks = 0, brackets = 0, doublesteps = 0,
		}
		if annotation.tech ~= nil then
			for tech in ivalues(annotation.tech) do
				local k = tech_key[ToEnumShortString(tech)]
				if k then tech_counts[k] = tech_counts[k] + 1 end
			end
		end

		out[#out + 1] = {
			beat = annotation.beat,
			columns = columns,
			feet = feet,
			note_count = note_count,
			tech_counts = tech_counts,
		}
	end

	return out
end

-- Export under a global the harness can pick up, and also return it so the file
-- can be `dofile`/`require`-d.
NoteAnnotationsForSteps = note_annotations_for_steps
return note_annotations_for_steps
