from selenium.webdriver.common.by import By


def test_main_window_appears(oca_driver):
    assert oca_driver.find_elements(By.CSS_SELECTOR, '[data-testid="screen-home"]')
