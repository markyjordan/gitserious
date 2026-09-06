use super::*;

fn focus(
    session: &mut AuthoringSession<'_>,
    heading: &str,
    occurrence: usize,
) -> Result<(), Box<dyn Error>> {
    let row = session
        .composer
        .editor
        .lines()
        .iter()
        .enumerate()
        .filter(|(_, line)| line.as_str() == heading)
        .nth(occurrence)
        .map(|(row, _)| row + 1)
        .ok_or("missing occurrence")?;
    session
        .composer
        .editor
        .move_cursor(CursorMove::Jump(u16::try_from(row)?, 0));
    Ok(())
}

#[test]
fn repeated_values_support_add_remove_undo_and_ordered_review() -> Result<(), Box<dyn Error>> {
    let definitions = vec![repeatable_definition()?];
    let mut session = AuthoringSession::new(&definitions, Some(0));
    focus(&mut session, "description:", 0)?;
    paste(&mut session, "collect evidence");
    focus(&mut session, "evidence:", 0)?;
    paste(&mut session, "first");
    modified_press(&mut session, KeyCode::Char('='), KeyModifiers::ALT);
    paste(&mut session, "second\ncontinued");
    modified_press(&mut session, KeyCode::Char('='), KeyModifiers::ALT);
    paste(&mut session, "third");
    assert!(rendered(&mut session, 100, 40)?.contains("alt+="));
    let original = session.composer.editor.lines().to_vec();
    modified_press(&mut session, KeyCode::Char('-'), KeyModifiers::ALT);
    assert!(
        !session
            .composer
            .editor
            .lines()
            .iter()
            .any(|line| line == "third")
    );
    modified_press(&mut session, KeyCode::Char('u'), KeyModifiers::CONTROL);
    assert_eq!(session.composer.editor.lines(), original);
    modified_press(&mut session, KeyCode::Char('r'), KeyModifiers::CONTROL);
    assert!(
        !session
            .composer
            .editor
            .lines()
            .iter()
            .any(|line| line == "third")
    );
    modified_press(&mut session, KeyCode::Char('u'), KeyModifiers::CONTROL);
    assert_eq!(session.composer.editor.lines(), original);
    focus(&mut session, "evidence:", 1)?;
    modified_press(&mut session, KeyCode::Char('-'), KeyModifiers::ALT);
    modified_press(&mut session, KeyCode::Char('s'), KeyModifiers::CONTROL);
    assert_eq!(session.stage, Stage::Review);
    assert_eq!(
        session
            .review
            .as_ref()
            .ok_or("missing review")?
            .message
            .as_str(),
        "custom: collect evidence\n\nevidence:\nfirst\n\nevidence:\nthird\n"
    );
    press(&mut session, KeyCode::Esc);
    focus(&mut session, "evidence:", 0)?;
    modified_press(&mut session, KeyCode::Char('-'), KeyModifiers::ALT);
    modified_press(&mut session, KeyCode::Char('-'), KeyModifiers::ALT);
    assert_eq!(
        session
            .composer
            .editor
            .lines()
            .iter()
            .filter(|line| line.as_str() == "evidence:")
            .count(),
        1
    );
    modified_press(&mut session, KeyCode::Char('s'), KeyModifiers::CONTROL);
    assert_eq!(session.stage, Stage::Compose);
    Ok(())
}

#[test]
fn custom_editor_label_collisions_keep_distinct_field_identity() -> Result<(), Box<dyn Error>> {
    let definitions = vec![CommitTypeDefinition::new(
        SchemaVersion::V1,
        CommitTypeId::new("custom")?,
        "Collision coverage",
        vec![
            property(
                "scope",
                PropertyRequirement::Required,
                PropertyMultiplicity::Multiple,
            )?,
            property(
                "description",
                PropertyRequirement::Required,
                PropertyMultiplicity::Single,
            )?,
            property(
                "breaking-change",
                PropertyRequirement::Required,
                PropertyMultiplicity::Single,
            )?,
        ],
    )?];
    let mut session = AuthoringSession::new(&definitions, Some(0));
    paste(&mut session, "header");
    focus(&mut session, "description:", 0)?;
    paste(&mut session, "distinct fields");
    focus(&mut session, "property[scope]:", 0)?;
    paste(&mut session, "body scope one");
    modified_press(&mut session, KeyCode::Char('='), KeyModifiers::ALT);
    paste(&mut session, "body scope two");
    focus(&mut session, "property[description]:", 0)?;
    paste(&mut session, "body description");
    let before = session.composer.editor.lines().to_vec();
    modified_press(&mut session, KeyCode::Char('='), KeyModifiers::ALT);
    modified_press(&mut session, KeyCode::Char('-'), KeyModifiers::ALT);
    assert_eq!(session.composer.editor.lines(), before);
    focus(&mut session, "property[breaking-change]:", 0)?;
    paste(&mut session, "body breaking context");
    focus(&mut session, "breaking-change:", 0)?;
    paste(&mut session, "footer migration");
    modified_press(&mut session, KeyCode::Char('s'), KeyModifiers::CONTROL);
    assert_eq!(session.stage, Stage::Review);
    assert_eq!(
        session
            .review
            .as_ref()
            .ok_or("missing review")?
            .message
            .as_str(),
        "custom(header)!: distinct fields\n\nscope:\nbody scope one\n\nscope:\nbody scope two\n\ndescription:\nbody description\n\nbreaking-change:\nbody breaking context\n\nBREAKING CHANGE: footer migration\n"
    );
    Ok(())
}
