use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TreatmentGroup {
    pub factor: String,
    pub duration: String,
    pub well_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WellAssignment {
    pub position: String,
    pub factor: String,
    pub duration: String,
    pub group_index: usize,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConditionGroup {
    pub condition: String,
    #[serde(default)]
    pub dose: String,
    #[serde(default)]
    pub duration: String,
    pub sample_count: usize,
}

#[derive(Debug, Clone)]
pub struct ConditionAssignment {
    pub condition: String,
    pub dose: String,
    pub duration: String,
    pub group_index: usize,
    pub replicate_index: usize,
    pub plate_position: Option<String>,
}

pub fn supported_capacity(value: &str) -> Option<usize> {
    for (label, capacity) in [
        ("三百八十四孔", 384),
        ("九十六孔", 96),
        ("四十八孔", 48),
        ("二十四孔", 24),
        ("十二孔", 12),
        ("六孔", 6),
    ] {
        if value.contains(label) {
            return Some(capacity);
        }
    }
    let digits = value
        .trim_start_matches(|character: char| !character.is_ascii_digit())
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>();
    match digits.parse::<usize>().ok()? {
        capacity @ (6 | 12 | 24 | 48 | 96 | 384) => Some(capacity),
        _ => None,
    }
}

pub fn capacity_from_metadata(metadata: &Value) -> Option<usize> {
    [
        "plate_capacity",
        "plate_format",
        "new_plate_format",
        "container_name",
    ]
    .iter()
    .filter_map(|key| metadata.get(key))
    .find_map(|value| {
        value
            .as_u64()
            .and_then(|number| supported_capacity(&number.to_string()))
            .or_else(|| value.as_str().and_then(supported_capacity))
    })
}

pub fn parse_and_assign(raw: &str, capacity: usize) -> Result<Vec<WellAssignment>, String> {
    if !matches!(capacity, 6 | 12 | 24 | 48 | 96 | 384) {
        return Err("Unsupported plate capacity; choose 6, 12, 24, 48, 96, or 384 wells".into());
    }
    let groups: Vec<TreatmentGroup> =
        serde_json::from_str(raw).map_err(|_| "Plate treatment groups are invalid".to_string())?;
    if groups.is_empty() {
        return Err("Add at least one plate treatment group".into());
    }
    for group in &groups {
        if group.factor.trim().is_empty() || group.duration.trim().is_empty() {
            return Err("Every plate group requires a treatment factor and duration".into());
        }
        if group.well_count == 0 {
            return Err("Every plate group must contain at least one well".into());
        }
    }
    let requested = groups.iter().try_fold(0usize, |total, group| {
        total
            .checked_add(group.well_count)
            .ok_or_else(|| "Plate well count is too large".to_string())
    })?;
    if requested > capacity {
        return Err(format!(
            "Treatment groups request {requested} wells, exceeding the {capacity}-well plate"
        ));
    }
    let positions = well_positions(capacity);
    let mut assignments = Vec::with_capacity(requested);
    for (group_index, group) in groups.into_iter().enumerate() {
        for _ in 0..group.well_count {
            let position = positions[assignments.len()].clone();
            assignments.push(WellAssignment {
                position,
                factor: group.factor.trim().to_owned(),
                duration: group.duration.trim().to_owned(),
                group_index,
            });
        }
    }
    Ok(assignments)
}

pub fn summary(assignments: &[WellAssignment]) -> String {
    let mut lines: Vec<String> = Vec::new();
    for assignment in assignments {
        if let Some(line) = lines.get_mut(assignment.group_index) {
            line.push_str(&format!(", {}", assignment.position));
        } else {
            lines.push(format!(
                "{}. {} / {}：{}",
                assignment.group_index + 1,
                assignment.factor,
                assignment.duration,
                assignment.position
            ));
        }
    }
    lines.join("\n")
}

pub fn parse_condition_groups(
    raw: &str,
    plate_capacity: Option<usize>,
) -> Result<Vec<ConditionAssignment>, String> {
    let groups: Vec<ConditionGroup> =
        serde_json::from_str(raw).map_err(|_| "Condition groups are invalid".to_string())?;
    if groups.is_empty() {
        return Err("Add at least one condition group".into());
    }
    for group in &groups {
        if group.condition.trim().is_empty() {
            return Err("Every condition group requires a condition".into());
        }
        if group.sample_count == 0 {
            return Err("Every condition group must produce at least one Sample".into());
        }
    }
    let requested = groups.iter().try_fold(0usize, |total, group| {
        total
            .checked_add(group.sample_count)
            .ok_or_else(|| "Condition group Sample count is too large".to_string())
    })?;
    let limit = plate_capacity.unwrap_or(384);
    if requested > limit {
        return Err(if plate_capacity.is_some() {
            format!(
                "Condition groups request {requested} positions, exceeding the {limit}-well plate"
            )
        } else {
            "Condition groups may produce at most 384 Samples per input".into()
        });
    }
    let positions = plate_capacity.map(well_positions);
    let mut assignments = Vec::with_capacity(requested);
    for (group_index, group) in groups.into_iter().enumerate() {
        for replicate_index in 1..=group.sample_count {
            assignments.push(ConditionAssignment {
                condition: group.condition.trim().to_owned(),
                dose: group.dose.trim().to_owned(),
                duration: group.duration.trim().to_owned(),
                group_index,
                replicate_index,
                plate_position: positions
                    .as_ref()
                    .and_then(|items| items.get(assignments.len()))
                    .cloned(),
            });
        }
    }
    Ok(assignments)
}

pub fn condition_summary(assignments: &[ConditionAssignment]) -> String {
    let mut lines: Vec<String> = Vec::new();
    for assignment in assignments {
        let details = [assignment.dose.as_str(), assignment.duration.as_str()]
            .into_iter()
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>()
            .join(" / ");
        if let Some(line) = lines.get_mut(assignment.group_index) {
            if let Some(position) = &assignment.plate_position {
                line.push_str(&format!(", {position}"));
            }
        } else {
            let suffix = if details.is_empty() {
                String::new()
            } else {
                format!(" / {details}")
            };
            let allocation = assignment
                .plate_position
                .as_ref()
                .map(|position| position.to_owned())
                .unwrap_or_else(|| {
                    format!(
                        "{} 个 Sample",
                        assignments
                            .iter()
                            .filter(|item| item.group_index == assignment.group_index)
                            .count()
                    )
                });
            lines.push(format!(
                "{}. {}{}：{}",
                assignment.group_index + 1,
                assignment.condition,
                suffix,
                allocation
            ));
        }
    }
    lines.join("\n")
}

pub fn well_positions(capacity: usize) -> Vec<String> {
    let (rows, columns) = match capacity {
        6 => (2, 3),
        12 => (3, 4),
        24 => (4, 6),
        48 => (6, 8),
        96 => (8, 12),
        384 => (16, 24),
        _ => unreachable!("capacity was validated"),
    };
    (0..rows)
        .flat_map(|row| {
            (1..=columns)
                .map(move |column| format!("{}{:02}", char::from(b'A' + row as u8), column))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn assigns_six_well_groups_in_row_major_order() {
        let assignments = parse_and_assign(
            r#"[{"factor":"si NC","duration":"24h","wellCount":3},{"factor":"si 123","duration":"24h","wellCount":3}]"#,
            6,
        )
        .unwrap();
        assert_eq!(assignments.len(), 6);
        assert_eq!(assignments[0].position, "A01");
        assert_eq!(assignments[2].position, "A03");
        assert_eq!(assignments[3].position, "B01");
        assert_eq!(assignments[5].position, "B03");
        assert_eq!(assignments[5].factor, "si 123");
    }

    #[test]
    fn rejects_groups_larger_than_plate() {
        let error = parse_and_assign(
            r#"[{"factor":"si NC","duration":"24h","wellCount":4},{"factor":"si 123","duration":"24h","wellCount":3}]"#,
            6,
        )
        .unwrap_err();
        assert!(error.contains("exceeding the 6-well plate"));
    }

    #[test]
    fn reads_capacity_without_putting_details_in_sample_id() {
        assert_eq!(
            capacity_from_metadata(&json!({"plate_format":"6孔板"})),
            Some(6)
        );
        assert_eq!(
            capacity_from_metadata(&json!({"container_name":"96孔板-1"})),
            Some(96)
        );
        assert_eq!(
            capacity_from_metadata(&json!({"container_name":"六孔板"})),
            Some(6)
        );
    }

    #[test]
    fn condition_groups_can_map_any_output_sample_to_plate_positions() {
        let assignments = parse_condition_groups(
            r#"[{"condition":"Control","sampleCount":2},{"condition":"Drug","dose":"10 nM","duration":"24 h","sampleCount":1}]"#,
            Some(6),
        )
        .unwrap();
        assert_eq!(assignments.len(), 3);
        assert_eq!(assignments[0].plate_position.as_deref(), Some("A01"));
        assert_eq!(assignments[2].plate_position.as_deref(), Some("A03"));
        assert_eq!(assignments[2].condition, "Drug");
        assert_eq!(assignments[2].replicate_index, 1);
    }
}
