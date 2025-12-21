#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteKind {
    Tap,
    Hold,
    Roll,
    Mine,
    Fake,
}

#[derive(Debug, Clone)]
pub struct ParsedNote {
    pub row_index: usize,
    pub column: usize,
    pub note_kind: NoteKind,
    pub tail_row_index: Option<usize>,
}

/// Parses minimized chart note data into note events, tracking hold/roll tails.
pub fn parse_chart_notes(minimized_note_data: &[u8], lanes: usize) -> Vec<ParsedNote> {
    let mut notes = Vec::new();
    let mut row_index = 0usize;
    let lanes = lanes.max(1);
    let mut hold_heads: Vec<Option<usize>> = vec![None; lanes];

    for line in minimized_note_data.split(|&b| b == b'\n') {
        let trimmed_line = line.strip_suffix(b"\r").unwrap_or(line);
        if trimmed_line.is_empty() || trimmed_line == b"," {
            continue;
        }

        if trimmed_line.len() >= lanes {
            for (col_index, &ch) in trimmed_line.iter().take(lanes).enumerate() {
                match ch {
                    b'1' => notes.push(ParsedNote {
                        row_index,
                        column: col_index,
                        note_kind: NoteKind::Tap,
                        tail_row_index: None,
                    }),
                    b'F' | b'f' => notes.push(ParsedNote {
                        row_index,
                        column: col_index,
                        note_kind: NoteKind::Fake,
                        tail_row_index: None,
                    }),
                    b'2' | b'4' => {
                        let note_kind = if ch == b'2' {
                            NoteKind::Hold
                        } else {
                            NoteKind::Roll
                        };
                        let note_index = notes.len();
                        notes.push(ParsedNote {
                            row_index,
                            column: col_index,
                            note_kind,
                            tail_row_index: None,
                        });
                        hold_heads[col_index] = Some(note_index);
                    }
                    b'M' | b'm' => notes.push(ParsedNote {
                        row_index,
                        column: col_index,
                        note_kind: NoteKind::Mine,
                        tail_row_index: None,
                    }),
                    b'3' => {
                        if let Some(head_idx) = hold_heads[col_index].take()
                            && let Some(note) = notes.get_mut(head_idx)
                        {
                            note.tail_row_index = Some(row_index);
                        }
                    }
                    _ => {}
                }
            }
        }
        row_index += 1;
    }

    notes
}
