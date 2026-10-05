# The egui shell had a single persistent nav rail with five destinations (Editor/Mídia/Fila/
# Ajustes/Início as separate top-level screens). The desktop shell's navigation model is
# different (see spec/architecture/tauri-migration.md): the topbar only ever switches between
# Home, Preferences and the Watch-folder screen; the editor (media library, timeline, export
# queue) is one screen reached by opening/creating a project, not three separate destinations.
# This test asserts the equivalent guarantee -- every topbar destination is reachable and
# renders its own screen -- against that real structure instead of the retired one.
from selenium.webdriver.common.by import By

from conftest import poll_until
from helpers import goto_editor, invoke

def test_home_opens_preferences_as_a_dialog(oca_driver):
    # Preferences and the watch folder are dialogs over Home now, opened from its header.
    oca_driver.find_element(By.CSS_SELECTOR, '[data-testid="home-settings"]').click()
    found = poll_until(
        lambda: oca_driver.find_elements(By.CSS_SELECTOR, '[role="dialog"]'),
        timeout_secs=3.0,
    )
    assert found, "no dialog after clicking Preferences on Home"


def test_home_opens_the_watch_folder(oca_driver):
    oca_driver.find_element(By.CSS_SELECTOR, '[data-testid="home-watch"]').click()
    found = poll_until(
        lambda: oca_driver.find_elements(By.CSS_SELECTOR, '[data-testid="screen-watch"]'),
        timeout_secs=3.0,
    )
    assert found, "no [data-testid=screen-watch] after clicking Watch folder on Home"


def test_new_project_asks_for_a_name_first(oca_driver):
    # The save dialog that follows is native, which WebDriver cannot drive, so the flow is
    # checked up to the name dialog and the editor is entered through the command itself.
    oca_driver.find_element(By.CSS_SELECTOR, '[data-testid="new-project"]').click()

    found = poll_until(
        lambda: oca_driver.find_elements(By.CSS_SELECTOR, '[role="dialog"] input[type="text"]'),
        timeout_secs=5.0,
    )
    assert found, "no name dialog after clicking New project"


def test_the_editor_opens_for_a_project_created_through_the_command(oca_driver):
    invoke(oca_driver, "new_project", {"name": "E2E", "sequenceName": "Seq"})
    goto_editor(oca_driver)

    found = poll_until(
        lambda: oca_driver.find_elements(By.CSS_SELECTOR, '[data-testid="screen-editor"]'),
        timeout_secs=5.0,
    )
    assert found, "no [data-testid=screen-editor] after creating a project"
