use std::collections::BTreeSet;

use ratatui_core::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{StatefulWidget, Widget},
};
use ratatui_widgets::{
    block::Block,
    borders::Borders,
    paragraph::{Paragraph, Wrap},
};

const DEFAULT_INPUT_HEIGHT: u16 = 5;
const MIN_INPUT_HEIGHT: u16 = 2;
const SELECTED_SEPARATOR: &str = " | ";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TagPickerFocus {
    Input,
    SelectedTags,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagPicker {
    available_tags: Vec<String>,
    input_height: u16,
    accent_color: Color,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagPickerState {
    selected_indices: Vec<usize>,
    focus: TagPickerFocus,
    input: String,
    match_cursor: usize,
    selected_cursor: usize,
    selected_scroll_x: usize,
}

impl Default for TagPickerState {
    fn default() -> Self {
        Self {
            selected_indices: Vec::new(),
            focus: TagPickerFocus::Input,
            input: String::new(),
            match_cursor: 0,
            selected_cursor: 0,
            selected_scroll_x: 0,
        }
    }
}

pub struct TagPickerConfig {
    pub input_height: u16,
    pub accent_color: Color,
}

impl Default for TagPickerConfig {
    fn default() -> Self {
        Self {
            input_height: DEFAULT_INPUT_HEIGHT,
            accent_color: Color::Yellow,
        }
    }
}

impl TagPicker {
    pub fn new<I, S>(available_tags: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self::with_config(available_tags, TagPickerConfig::default())
    }

    pub fn with_config<I, S>(available_tags: I, config: TagPickerConfig) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            available_tags: normalize_available_tags(available_tags),
            input_height: config.input_height.max(MIN_INPUT_HEIGHT),
            accent_color: config.accent_color,
        }
    }

    pub fn set_available_tags<I, S>(&mut self, available_tags: I)
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.available_tags = normalize_available_tags(available_tags);
    }

    fn tag(&self, index: usize) -> Option<&str> {
        self.available_tags.get(index).map(String::as_str)
    }

    fn matched_tag_indices(&self, state: &TagPickerState) -> Vec<usize> {
        let selected_indices = state
            .selected_indices
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        let mut matches = self
            .available_tags
            .iter()
            .enumerate()
            .filter(|(index, _)| !selected_indices.contains(index))
            .filter_map(|(index, tag)| fuzzy_score(&state.input, tag).map(|score| (score, index)))
            .collect::<Vec<_>>();

        matches.sort_unstable_by(|(score_a, index_a), (score_b, index_b)| {
            let tag_a = &self.available_tags[*index_a];
            let tag_b = &self.available_tags[*index_b];
            score_b
                .cmp(score_a)
                .then_with(|| tag_a.to_lowercase().cmp(&tag_b.to_lowercase()))
        });

        matches.into_iter().map(|(_, index)| index).collect()
    }

    fn retain_selected_indices(&self, state: &mut TagPickerState) {
        let mut seen = BTreeSet::new();
        state
            .selected_indices
            .retain(|index| *index < self.available_tags.len() && seen.insert(*index));
    }

    fn sync_render_state(&self, state: &mut TagPickerState) {
        self.retain_selected_indices(state);

        let match_count = self.matched_tag_indices(state).len();
        state.match_cursor = if match_count == 0 {
            0
        } else {
            state.match_cursor.min(match_count - 1)
        };

        state.selected_cursor = if state.selected_indices.is_empty() {
            state.selected_scroll_x = 0;
            0
        } else {
            state.selected_cursor.min(state.selected_indices.len() - 1)
        };
    }

    fn valid_selected_raw_positions(&self, state: &TagPickerState) -> Vec<usize> {
        state
            .selected_indices
            .iter()
            .enumerate()
            .filter_map(|(position, &tag_index)| self.tag(tag_index).map(|_| position))
            .collect()
    }

    fn render_input_area(&self, state: &TagPickerState, area: Rect, buf: &mut Buffer) {
        let block = Block::default().borders(Borders::BOTTOM);
        let inner = block.inner(area);
        block.render(area, buf);

        let matches = self.matched_tag_indices(state);
        let mut lines = vec![Line::from(vec![
            Span::styled("> ", Style::new().fg(self.accent_color)),
            Span::raw(if state.input.is_empty() {
                "<type to search>".to_string()
            } else {
                state.input.clone()
            }),
        ])];

        let preview_limit = inner.height.saturating_sub(1) as usize;
        if preview_limit > 0 {
            if matches.is_empty() {
                lines.push(Line::from(Span::styled(
                    "No matching tags",
                    Style::new().fg(Color::DarkGray),
                )));
            } else {
                for row in visible_match_rows(matches.len(), state.match_cursor, preview_limit) {
                    match row {
                        MatchRow::EllipsisBelow => {
                            lines.push(Line::from(Span::styled(
                                "...",
                                Style::new().fg(Color::DarkGray),
                            )));
                        }
                        MatchRow::Item(index) => {
                            let Some(tag) = self.tag(matches[index]) else {
                                continue;
                            };
                            let style = if index == state.match_cursor
                                && state.focus == TagPickerFocus::Input
                            {
                                Style::new().fg(Color::Black).bg(self.accent_color)
                            } else {
                                Style::new().fg(Color::DarkGray)
                            };
                            lines.push(Line::from(Span::styled(format!("{tag}"), style)));
                        }
                    }
                }
            }
        }

        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .render(inner, buf);
    }

    fn render_selected_area(&self, state: &mut TagPickerState, area: Rect, buf: &mut Buffer) {
        let line = if state.selected_indices.is_empty() {
            Line::from(Span::styled(
                "No tags selected",
                Style::new().fg(Color::DarkGray),
            ))
        } else {
            let mut spans = Vec::new();
            let mut selected_bounds = None;
            let mut line_width = 0;

            for (index, tag_index) in state.selected_indices.iter().copied().enumerate() {
                let Some(tag) = self.tag(tag_index) else {
                    continue;
                };

                if index > 0 {
                    spans.push(Span::raw(SELECTED_SEPARATOR));
                    line_width += SELECTED_SEPARATOR.chars().count();
                }

                let is_selected = index == state.selected_cursor;
                if is_selected {
                    let text = format!("{tag}");
                    let separator_width = SELECTED_SEPARATOR.chars().count();
                    let start = if index > 0 {
                        line_width.saturating_sub(separator_width)
                    } else {
                        line_width
                    };
                    let mut end = line_width + text.chars().count();
                    if index + 1 < state.selected_indices.len() {
                        end += separator_width;
                    }
                    selected_bounds = Some((start, end));
                    let style = if state.focus == TagPickerFocus::SelectedTags {
                        Style::new().fg(Color::Black).bg(self.accent_color)
                    } else {
                        Style::new().fg(Color::default())
                    };
                    line_width += text.chars().count();
                    spans.push(Span::styled(text, style));
                } else {
                    line_width += tag.chars().count();
                    spans.push(Span::raw(tag));
                }
            }

            sync_scroll_to_visible(
                &mut state.selected_scroll_x,
                area.width as usize,
                line_width,
                selected_bounds,
            );
            Line::from(spans)
        };

        Paragraph::new(vec![line])
            .scroll((0, state.selected_scroll_x.min(u16::MAX as usize) as u16))
            .render(area, buf);
    }
}

impl TagPickerState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn new_with_selected_tags<I, S>(picker: &TagPicker, selected_tags: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut state = Self::new();
        state.set_selected_tags(picker, selected_tags);
        state
    }

    pub fn selected_indices(&self) -> &[usize] {
        &self.selected_indices
    }

    pub fn selected_tags<'a>(
        &'a self,
        picker: &'a TagPicker,
    ) -> impl Iterator<Item = &'a str> + 'a {
        self.selected_indices
            .iter()
            .filter_map(|&index| picker.tag(index))
    }

    pub fn cycle_focus(&mut self) {
        self.focus = match self.focus {
            TagPickerFocus::Input => TagPickerFocus::SelectedTags,
            TagPickerFocus::SelectedTags => TagPickerFocus::Input,
        };
    }

    pub fn focus_input(&mut self) {
        self.focus = TagPickerFocus::Input;
    }

    pub fn focus_selected_tags(&mut self) {
        self.focus = TagPickerFocus::SelectedTags;
    }

    pub fn clear_input(&mut self) {
        self.input.clear();
        self.match_cursor = 0;
    }

    pub fn insert_char(&mut self, ch: char) {
        if self.focus != TagPickerFocus::Input || ch.is_control() {
            return;
        }

        self.input.push(ch);
        self.match_cursor = 0;
    }

    pub fn backspace(&mut self) {
        if self.focus != TagPickerFocus::Input {
            return;
        }

        self.input.pop();
        self.match_cursor = 0;
    }

    pub fn move_next(&mut self, picker: &TagPicker) {
        match self.focus {
            TagPickerFocus::Input => {
                let match_count = picker.matched_tag_indices(self).len();
                if match_count > 0 {
                    self.match_cursor = (self.match_cursor + 1) % match_count;
                }
            }
            TagPickerFocus::SelectedTags => {
                let selected_count = picker.valid_selected_raw_positions(self).len();
                if selected_count > 0 {
                    let cursor = self.selected_cursor.min(selected_count - 1);
                    self.selected_cursor = (cursor + 1) % selected_count;
                }
            }
        }
    }

    pub fn move_previous(&mut self, picker: &TagPicker) {
        match self.focus {
            TagPickerFocus::Input => {
                let match_count = picker.matched_tag_indices(self).len();
                if match_count > 0 {
                    self.match_cursor = if self.match_cursor == 0 {
                        match_count - 1
                    } else {
                        self.match_cursor - 1
                    };
                }
            }
            TagPickerFocus::SelectedTags => {
                let selected_count = picker.valid_selected_raw_positions(self).len();
                if selected_count > 0 {
                    let cursor = self.selected_cursor.min(selected_count - 1);
                    self.selected_cursor = if cursor == 0 {
                        selected_count - 1
                    } else {
                        cursor - 1
                    };
                }
            }
        }
    }

    pub fn confirm(&mut self, picker: &TagPicker) {
        if self.focus != TagPickerFocus::Input {
            return;
        }

        let matches = picker.matched_tag_indices(self);
        let Some(selected_index) = matches.get(self.match_cursor).copied() else {
            return;
        };
        if self.selected_indices.contains(&selected_index) {
            return;
        }

        self.selected_indices.push(selected_index);
        self.selected_cursor = self.selected_indices.len().saturating_sub(1);
        self.input.clear();
        self.match_cursor = 0;
    }

    pub fn remove_selected_tag(&mut self, picker: &TagPicker) {
        if self.focus != TagPickerFocus::SelectedTags || self.selected_indices.is_empty() {
            return;
        }

        let valid_positions = picker.valid_selected_raw_positions(self);
        let Some(raw_index) = valid_positions
            .get(
                self.selected_cursor
                    .min(valid_positions.len().saturating_sub(1)),
            )
            .copied()
        else {
            return;
        };

        self.selected_indices.remove(raw_index);
        let remaining_count = valid_positions.len().saturating_sub(1);
        if remaining_count == 0 {
            self.selected_cursor = 0;
            self.selected_scroll_x = 0;
        } else {
            self.selected_cursor = self.selected_cursor.min(remaining_count - 1);
        }
    }

    fn set_selected_tags<I, S>(&mut self, picker: &TagPicker, selected_tags: I)
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.selected_indices.clear();
        let mut seen = BTreeSet::new();

        for tag in selected_tags {
            let tag = tag.into();
            let Some(index) = picker
                .available_tags
                .iter()
                .position(|candidate| candidate == &tag)
            else {
                continue;
            };

            if seen.insert(index) {
                self.selected_indices.push(index);
            }
        }

        self.selected_cursor = 0;
        self.selected_scroll_x = 0;
    }
}

impl StatefulWidget for &TagPicker {
    type State = TagPickerState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        self.sync_render_state(state);

        let outer = Block::default().borders(Borders::ALL).title("Tags");
        let inner = outer.inner(area);
        outer.render(area, buf);

        let sections = Layout::vertical([
            Constraint::Length(self.input_height.saturating_add(2)),
            Constraint::Length(3),
        ])
        .split(inner);
        self.render_input_area(state, sections[0], buf);
        self.render_selected_area(state, sections[1], buf);
    }
}

fn normalize_available_tags<I, S>(available_tags: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    available_tags
        .into_iter()
        .map(Into::into)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn sync_scroll_to_visible(
    scroll_x: &mut usize,
    viewport_width: usize,
    content_width: usize,
    selected_bounds: Option<(usize, usize)>,
) {
    if viewport_width == 0 || content_width <= viewport_width {
        *scroll_x = 0;
        return;
    }

    let max_scroll = content_width - viewport_width;
    *scroll_x = (*scroll_x).min(max_scroll);

    let Some((start, end)) = selected_bounds else {
        return;
    };

    if end.saturating_sub(start) >= viewport_width {
        *scroll_x = start.min(max_scroll);
    } else if end > *scroll_x + viewport_width {
        *scroll_x = (end - viewport_width).min(max_scroll);
    } else if start < *scroll_x {
        *scroll_x = start;
    }
}

fn fuzzy_score(query: &str, candidate: &str) -> Option<i64> {
    if query.is_empty() {
        return Some(0);
    }

    let query = query.to_lowercase();
    let candidate = candidate.to_lowercase();

    let mut score = 0_i64;
    let mut search_from = 0_usize;
    let mut previous_match = None;

    for query_char in query.chars() {
        let rest = candidate.get(search_from..)?;
        let offset = rest.find(query_char)?;
        let match_index = search_from + offset;

        score += 10;
        score -= offset as i64;

        if match_index == 0 {
            score += 8;
        }

        if let Some(previous_match) = previous_match {
            if match_index == previous_match + 1 {
                score += 6;
            }
        }

        previous_match = Some(match_index);
        search_from = match_index + query_char.len_utf8();
    }

    Some(score)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MatchRow {
    Item(usize),
    EllipsisBelow,
}

fn visible_match_rows(match_count: usize, cursor: usize, max_rows: usize) -> Vec<MatchRow> {
    if match_count == 0 || max_rows == 0 {
        return Vec::new();
    }

    let cursor = cursor.min(match_count - 1);

    if match_count <= max_rows {
        return (0..match_count).map(MatchRow::Item).collect();
    }

    if max_rows == 1 {
        return vec![MatchRow::EllipsisBelow];
    }

    let visible_items = max_rows - 1;
    let mut start = cursor.saturating_sub(visible_items.saturating_sub(1));
    let mut end = start + visible_items;

    if end >= match_count {
        end = match_count;
        start = end.saturating_sub(max_rows);
    }

    let mut rows = (start..end).map(MatchRow::Item).collect::<Vec<_>>();
    if end < match_count {
        rows.push(MatchRow::EllipsisBelow);
    }
    rows
}

#[cfg(test)]
mod tests {
    use ratatui::{buffer::Buffer, layout::Rect, widgets::StatefulWidget};

    use super::{
        MatchRow, TagPicker, TagPickerFocus, TagPickerState, fuzzy_score, sync_scroll_to_visible,
        visible_match_rows,
    };
    use crate::TagPickerConfig;

    #[test]
    fn fuzzy_score_prefers_prefix_and_contiguous_matches() {
        assert!(fuzzy_score("rs", "rust").unwrap() > fuzzy_score("rs", "crates").unwrap());
        assert!(fuzzy_score("tag", "tags").unwrap() > fuzzy_score("tag", "meta graph").unwrap());
    }

    #[test]
    fn input_focus_filters_and_confirms_a_match() {
        let picker = TagPicker::new(["rust", "ratatui", "ruby"]);
        let mut state = TagPickerState::new();

        state.insert_char('r');
        state.insert_char('a');

        assert_eq!(state.focus, TagPickerFocus::Input);
        assert_eq!(
            picker
                .matched_tag_indices(&state)
                .into_iter()
                .filter_map(|index| picker.tag(index))
                .collect::<Vec<_>>(),
            vec!["ratatui"]
        );

        state.confirm(&picker);
        assert_eq!(state.selected_indices(), &[0]);
        assert_eq!(
            state.selected_tags(&picker).collect::<Vec<_>>(),
            vec!["ratatui"]
        );
        assert_eq!(state.input, "");
    }

    #[test]
    fn state_with_selected_tags_applies_selection() {
        let picker = TagPicker::new(["rust", "ratatui", "ruby"]);
        let state = TagPickerState::new_with_selected_tags(&picker, ["rust", "ratatui"]);

        assert_eq!(
            state.selected_tags(&picker).collect::<Vec<_>>(),
            vec!["rust", "ratatui"]
        );
    }

    #[test]
    fn cycling_focus_and_removing_selected_tag_works() {
        let picker = TagPicker::new(["rust", "ratatui", "ruby"]);
        let mut state = TagPickerState::new_with_selected_tags(&picker, ["rust", "ratatui"]);

        state.cycle_focus();
        state.move_next(&picker);

        assert_eq!(state.focus, TagPickerFocus::SelectedTags);
        state.remove_selected_tag(&picker);
        assert_eq!(
            state.selected_tags(&picker).collect::<Vec<_>>(),
            vec!["rust"]
        );
    }

    #[test]
    fn input_methods_do_nothing_when_selected_tags_are_focused() {
        let mut state = TagPickerState::new();

        state.cycle_focus();
        state.insert_char('r');
        state.backspace();

        assert_eq!(state.focus, TagPickerFocus::SelectedTags);
        assert_eq!(state.input, "");
    }

    #[test]
    fn constructor_clamps_input_height() {
        let picker = TagPicker::with_config(
            ["rust"],
            TagPickerConfig {
                input_height: 1,
                ..Default::default()
            },
        );

        assert_eq!(picker.input_height, 2);
    }

    #[test]
    fn overflowing_matches_show_ellipsis_and_scroll_with_cursor() {
        let rows = visible_match_rows(8, 4, 4);

        assert_eq!(
            rows,
            vec![
                MatchRow::Item(2),
                MatchRow::Item(3),
                MatchRow::Item(4),
                MatchRow::EllipsisBelow,
            ]
        );
    }

    #[test]
    fn rendering_overflowing_matches_shows_ellipsis() {
        let picker = TagPicker::with_config(
            [
                "tag-0", "tag-1", "tag-2", "tag-3", "tag-4", "tag-5", "tag-6", "tag-7",
            ],
            TagPickerConfig {
                input_height: 4,
                ..Default::default()
            },
        );
        let mut state = TagPickerState::new();
        state.insert_char('t');
        state.move_next(&picker);
        state.move_next(&picker);
        state.move_next(&picker);
        state.move_next(&picker);
        let mut buffer = Buffer::empty(Rect::new(0, 0, 40, 12));

        (&picker).render(buffer.area, &mut buffer, &mut state);

        let rendered = buffer
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();

        assert!(rendered.contains("..."));
        assert!(rendered.contains("tag-4"));
    }

    #[test]
    fn state_selection_is_pruned_when_available_tags_change() {
        let mut picker = TagPicker::new(["rust", "ratatui"]);
        let mut state = TagPickerState::new_with_selected_tags(&picker, ["rust", "ratatui"]);

        picker.set_available_tags(["rust"]);
        (&picker).render(
            Rect::new(0, 0, 50, 10),
            &mut Buffer::empty(Rect::new(0, 0, 50, 10)),
            &mut state,
        );

        assert_eq!(
            state.selected_tags(&picker).collect::<Vec<_>>(),
            vec!["rust"]
        );
    }

    #[test]
    fn selected_tags_scroll_horizontally_to_keep_selection_visible() {
        let picker = TagPicker::new(["alpha", "beta", "gamma"]);
        let mut state = TagPickerState::new_with_selected_tags(&picker, ["alpha", "beta", "gamma"]);
        state.cycle_focus();
        state.move_next(&picker);
        state.move_next(&picker);
        let mut buffer = Buffer::empty(Rect::new(0, 0, 20, 10));

        (&picker).render(buffer.area, &mut buffer, &mut state);

        assert!(state.selected_scroll_x > 0);
    }

    #[test]
    fn confirming_selects_the_newly_added_tag() {
        let picker = TagPicker::new(["alpha", "beta", "gamma"]);
        let mut state = TagPickerState::new_with_selected_tags(&picker, ["alpha"]);

        state.insert_char('g');
        state.confirm(&picker);

        assert_eq!(
            state.selected_tags(&picker).nth(state.selected_cursor),
            Some("gamma")
        );
    }

    #[test]
    fn scroll_sync_keeps_selection_in_view() {
        let mut scroll_x = 0;

        sync_scroll_to_visible(&mut scroll_x, 8, 22, Some((15, 22)));

        assert_eq!(scroll_x, 14);
    }
}
