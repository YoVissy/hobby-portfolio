#include <stdio.h>
#include <zephyr/kernel.h>
#include <zephyr/drivers/gpio.h>

#define AJASTIN_MS   500  // Aika (ms), jonka kahvinkeitin pysyy päällä ensimmäisen aamukytkennän jälkeen

// Laitteiston määrittelyt: valokytkin, kahvinkeitin ja muita älykodin laitteita
#define VALOKYTKIN_NODE DT_ALIAS(sw0)  // Valokytkin
#define KAHVINKEITIN_NODE DT_ALIAS(led0)  // Kahvinkeitin (rele tai LED-simulaatio)
#define VALAISTUS_NODE DT_ALIAS(led1)  // Älyvalaistus
#define LUKKO_NODE DT_ALIAS(led2)  // Älylukko
#define HUB_LED_NODE DT_ALIAS(led3) // HUB-led (syttyy, kun kahvinkeitin on päällä)
#define SW0_NODE DT_ALIAS(sw0)
#define SW2_NODE DT_ALIAS(sw2) // Button2 lämpötilan nostamiseen
#define SW4_NODE DT_ALIAS(sw4) // Button4 lämpötilan laskemiseen

// GPIO-määrittelyt
static const struct gpio_dt_spec hub_led = GPIO_DT_SPEC_GET(HUB_LED_NODE, gpios);
static const struct gpio_dt_spec button1 = GPIO_DT_SPEC_GET(SW0_NODE, gpios);
static const struct gpio_dt_spec button2 = GPIO_DT_SPEC_GET(SW2_NODE, gpios);
static const struct gpio_dt_spec button4 = GPIO_DT_SPEC_GET(SW4_NODE, gpios);
static const struct gpio_dt_spec valokytkin = GPIO_DT_SPEC_GET(VALOKYTKIN_NODE, gpios);
static const struct gpio_dt_spec kahvinkeitin = GPIO_DT_SPEC_GET(KAHVINKEITIN_NODE, gpios);
static const struct gpio_dt_spec valaistus = GPIO_DT_SPEC_GET(VALAISTUS_NODE, gpios);
static const struct gpio_dt_spec lukko = GPIO_DT_SPEC_GET(LUKKO_NODE, gpios);

int main(void)
{
    int ret;
    bool kahvinkeitin_paalla = false;
    int64_t sammutusaika = 0;
    bool aamun_ensimmainen_kerta = true;
    bool valaistus_paalla = false;
    int lampotila = 20; // Oletuslämpötila 20°C
    
    // Tarkistetaan, että laitteet ovat valmiita
    if (!gpio_is_ready_dt(&valokytkin) || !gpio_is_ready_dt(&kahvinkeitin) || !gpio_is_ready_dt(&valaistus) || !gpio_is_ready_dt(&lukko) || !gpio_is_ready_dt(&hub_led) || !gpio_is_ready_dt(&button2) || !gpio_is_ready_dt(&button4)) {
        return 0;
    }
    
    // Konfiguroidaan laitteet ulostuloiksi
    ret = gpio_pin_configure_dt(&kahvinkeitin, GPIO_OUTPUT_INACTIVE);
    ret |= gpio_pin_configure_dt(&valaistus, GPIO_OUTPUT_INACTIVE);
    ret |= gpio_pin_configure_dt(&lukko, GPIO_OUTPUT_INACTIVE);
    ret |= gpio_pin_configure_dt(&hub_led, GPIO_OUTPUT_INACTIVE);
    if (ret < 0) {
        return 0;
    }
    
    // Konfiguroidaan napit sisääntuloiksi
    ret = gpio_pin_configure_dt(&valokytkin, GPIO_INPUT);
    ret |= gpio_pin_configure_dt(&button1, GPIO_INPUT);
    ret |= gpio_pin_configure_dt(&button2, GPIO_INPUT);
    ret |= gpio_pin_configure_dt(&button4, GPIO_INPUT);
    if (ret < 0) {
        return 0;
    }
    
    while (1) {
        int64_t nykyinen_aika = k_uptime_get();
        int valokytkin_tila = gpio_pin_get_dt(&valokytkin);
        int button1_tila = gpio_pin_get_dt(&button1);
        int button2_tila = gpio_pin_get_dt(&button2);
        int button4_tila = gpio_pin_get_dt(&button4);

        // Jos valokytkin kytketään ensimmäistä kertaa päälle aamulla, käynnistetään kahvinkeitin ja muut älykodin laitteet
        if (valokytkin_tila && aamun_ensimmainen_kerta) {
            gpio_pin_set_dt(&kahvinkeitin, 1);
            gpio_pin_set_dt(&hub_led, 1);
            if (!valaistus_paalla) {
                gpio_pin_set_dt(&valaistus, 1);
                valaistus_paalla = true;
            }
            gpio_pin_set_dt(&lukko, 1);
            kahvinkeitin_paalla = true;
            sammutusaika = nykyinen_aika + AJASTIN_MS;
            aamun_ensimmainen_kerta = false;
        }

        // Sammutetaan kahvinkeitin
        if (kahvinkeitin_paalla && nykyinen_aika >= sammutusaika) {
            gpio_pin_set_dt(&kahvinkeitin, 0);
            gpio_pin_set_dt(&hub_led, 0);
            kahvinkeitin_paalla = false;
        }

        // Button1 voi kytkeä valaistuksen päälle tai pois päältä
        if (button1_tila) {
            valaistus_paalla = !valaistus_paalla;
            gpio_pin_set_dt(&valaistus, valaistus_paalla);
            k_msleep(300);
        }

        // Button2 nostaa lämpötilaa
        if (button2_tila) {
            lampotila++;
            printf("Lämpötila: %d°C\n", lampotila);
            k_msleep(300);
        }

        // Button4 laskee lämpötilaa
        if (button4_tila) {
            lampotila--;
            printf("Lämpötila: %d°C\n", lampotila);
            k_msleep(300);
        }

        k_msleep(10);
    }
    return 0;
}

// Tämä ohjelma on esimerkki älykodin laitteiden ohjauksesta Zephyr RTOS:lla.
// Se sisältää valokytkimen, kahvinkeittimen, älyvalaistuksen ja älylukon ohjauksen.
// Ohjelma tarkistaa laitteiden tilat ja reagoi käyttäjän syötteisiin.