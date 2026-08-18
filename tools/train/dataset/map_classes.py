#!/usr/bin/env python3
"""Map the 365 Places365 class names onto Open Images v7 class MIDs.

Fuzzy name matching (difflib) against OI's class descriptions, restricted to
OI 'trainable' classes. Outputs:
  tools/train/class-map.json        (committed; fine->coarse->mid)
  ml-corpus/openimages/metadata/map_review.txt  (gitignored; human review)

Coarse groups: a hand-maintained mapping of fine class -> broad category,
kept in this file as COARSE_GROUPS. The classifier learns two heads: the
coarse group (30-60 of these) and the fine scene (~250-330 classes).
"""
from __future__ import annotations

import argparse
import csv
import difflib
import json
import pathlib
import re

COARSE_GROUPS: dict[str, str] = {
    # nature / landscape
    "beach": "nature", "coast": "nature", "seashore": "nature", "lighthouse": "nature",
    "forest_path": "nature", "forest_road": "nature", "forest": "nature", "woods": "nature",
    "mountain": "nature", "volcano": "nature", "valley": "nature", "canyon": "nature",
    "desert_sand": "nature", "desert_road": "nature", "desert": "nature",
    "lake": "nature", "waterfall": "nature", "river": "nature", "pond": "nature",
    "harbor": "nature", "dock": "nature", "pier": "nature", "sandbar": "nature",
    "snowfield": "nature", "iceberg": "nature", "glacier": "nature", "ski_resort": "nature",
    "field_cultivated": "nature", "pasture": "nature", "hayfield": "nature",
    "rice_paddy": "nature", "vineyard": "nature", "orchard": "nature", "corn_field": "nature",
    "crop_field": "nature", "zen_garden": "nature", "formal_garden": "nature",
    "garden": "nature", "park": "nature", "greenhouse": "nature",
    "flower_shop": "indoor", "flower_field": "nature",
    "jungle": "nature", "rainforest": "nature", "swamp": "nature", "marsh": "nature",
    "wetland": "nature", "reed_bed": "nature", "moor": "nature", "heath": "nature",
    "bamboo_forest": "nature", "monastery": "religious", "herb_garden": "nature",
    "tree_farm": "nature", "timber_yard": "industrial", "bullring": "sports_stadium",
    "elevator_shaft": "indoor", "escalator_interior": "indoor",
    "alley": "urban", "arcade": "urban", "balcony": "urban", "plaza": "urban",
    "little_haiti": "urban", "bazaar_indoor": "urban", "market_indoor": "urban",
    "market_outdoor": "urban", "city_center": "urban", "city_street": "urban",
    "street": "urban", "streetcar_interior": "indoor", "sidewalk": "urban",
    "staircase": "indoor", "stairwell": "indoor", "subway_station": "public_transport",
    "bus_station": "public_transport", "train_station": "public_transport",
    "train_window": "public_transport", "airport_terminal": "public_transport",
    "airport_lounge": "public_transport", "airport_entrance": "public_transport",
    "heliport": "public_transport", "boat_deck": "nature_water", "dock": "nature",
    "gas_station": "urban", "parking_lot": "urban", "covered_parking": "urban",
    "garage_indoor": "indoor", "tunnel": "urban", "underpass": "urban",
    "bridge": "urban", "footbridge": "urban", "construction_site": "urban",
    "skyscraper": "urban", "office_tower": "urban", "office_building": "urban",
    "apartment_building": "urban", "residential_neighborhood": "residential",
    "house": "residential", "farmhouse": "residential", "cabin": "residential",
    "log_cabin": "residential", "lodge": "residential", "bungalow": "residential",
    "mobile_home": "residential", "tent": "residential", "cottage_garden": "residential",
    "ruin": "urban", "castle": "historic", "palace": "historic", "cathedral": "religious",
    "church": "religious", "chapel": "religious", "mosque": "religious",
    "synagogue": "religious", "temple": "religious", "pagoda": "religious",
    "basilica": "religious", "bell_tower": "religious", "monastery": "religious",
    "abbey": "religious", "mausoleum": "historic", "memorial": "historic",
    "monument": "historic", "archaeological_excavation": "historic",
    "museum": "indoor_cultural", "art_gallery": "indoor_cultural", "library": "indoor_cultural",
    "bookstore": "indoor_retail", "video_store": "indoor_retail", "music_store": "indoor_retail",
    "toyshop": "indoor_retail", "bicycle_store": "indoor_retail", "car_dealership": "indoor_retail",
    "clothing_store": "indoor_retail", "jewelry_shop": "indoor_retail", "shoeshine_stand": "urban",
    "pharmacy": "indoor_retail", "grocery_store": "indoor_retail", "supermarket": "indoor_retail",
    "convenience_store": "indoor_retail", "bakery_shop": "indoor_retail", "butcher_shop": "indoor_retail",
    "fish_market": "indoor_retail", "hardware_store": "indoor_retail", "florist_shop": "indoor_retail",
    "antique_shop": "indoor_retail", "furniture_store": "indoor_retail",
    "restaurant": "food_dining", "food_court": "food_dining", "cafeteria": "food_dining",
    "coffee_shop": "food_dining", "cafe": "food_dining", "tea_house": "food_dining",
    "bar": "food_dining", "pub": "food_dining", "cocktail_bar": "food_dining",
    "fastfood_restaurant": "food_dining", "pizzeria": "food_dining", "ice_cream_parlor": "food_dining",
    "deli": "food_dining", "kitchen": "indoor_home", "dining_room": "indoor_home",
    "hotel_room": "hotel", "hotel_lobby": "hotel", "bedroom": "indoor_home",
    "nursery_room": "indoor_home", "nursery": "indoor_home", "living_room": "indoor_home",
    "home_office": "indoor_home", "home_library": "indoor_home", "home_theater": "indoor_home",
    "bathroom": "indoor_home", "shower": "indoor_home", "sauna": "indoor_home",
    "jaccuzzi": "indoor_home", "pantry": "indoor_home", "basement": "indoor_home",
    "attic": "indoor_home", "garage": "indoor_home", "laundromat": "indoor_home",
    "ironing": "indoor_home", "closet": "indoor_home", "utility_room": "indoor_home",
    "storage_room": "indoor_home", "staircase": "indoor", "home_furniture_store": "indoor_retail",
    "classroom": "education", "schoolhouse": "education", "kindergarten": "education",
    "college_classroom": "education", "computer_room": "education", "laboratory": "education",
    "lantern": "urban_detail", "biology_laboratory": "education",
    "office": "workplace", "office_cubicles": "workplace", "office_shop": "workplace",
    "waiting_room": "indoor", "reception": "indoor", "conference_room": "workplace",
    "library": "indoor_cultural", "reading_room": "indoor_cultural", "study": "indoor_home",
    "courtyard": "urban", "palace": "historic", "hunting_lodge": "nature",
    "sports_field": "sports", "soccer_field": "sports", "playing_field": "sports",
    "baseball_field": "sports", "softball_field": "sports", "stadium": "sports_stadium",
    "athletic_field": "sports", "golf_course": "sports", "mini_golf_course": "sports",
    "tennis_court": "sports", "basketball_court": "sports", "ball_pit": "sports",
    "skating_rink": "sports", "ice_skating_rink": "sports", "pool": "sports",
    "swimming_pool": "sports", "water_park": "sports", "playground": "sports",
    "amusement_park": "sports", "carousel": "sports", "ferris_wheel": "sports",
    "roller_skating": "sports", "bowling_alley": "sports", "dance_floor": "sports",
    "bungee_jumping": "sports", "parade": "event", "fair": "event", "circus": "event",
    "festival_gathering": "event", "crowded_indoor": "event", "resort": "hotel",
    "movie_theater": "indoor_cultural", "theater": "indoor_cultural", "opera_house": "indoor_cultural",
    "amphitheater": "indoor_cultural", "concert_hall": "indoor_cultural", "nightclub": "food_dining",
    "discotique": "food_dining", "ballroom": "hotel", "church": "religious",
    "hospital": "healthcare", "dental_office": "healthcare", "operating_room": "healthcare",
    "doctor_station": "healthcare", "hospital_room": "healthcare", "veterinarians_office": "healthcare",
    "pharmacy": "indoor_retail", "daycare": "education", "nursery_room": "indoor_home",
    "prison_cell": "indoor", "jail_cell": "indoor", "courtroom": "workplace",
    "jail_interior": "indoor", "police_office": "workplace", "fire_escape": "urban",
    "fire_station": "workplace", "post_office": "workplace", "police_station": "workplace",
    "train_station": "public_transport", "subway_platform": "public_transport",
    "runway": "public_transport", "aircraft_cabin": "public_transport", "airport_control_tower": "public_transport",
    "hangar": "public_transport", "waiting_room": "indoor",
    "gas_station": "urban", "auto_showroom": "urban", "car_interior": "transport_vehicle",
    "bus_interior": "transport_vehicle", "boat_deck": "nature_water", "ship": "transport_vehicle",
    "submarine": "transport_vehicle", "tank": "transport_vehicle", "racecourse": "sports",
    "amusement_arcade": "sports", "go-kart_track": "sports", "sledding": "sports",
    "snowboarding": "sports", "ski_slope": "sports", "ski_resort": "nature",
    "zoo": "nature", "aquarium": "nature", "petshop": "nature", "bird_aviary": "nature",
    "stable": "nature", "horse_ranch": "nature", "kennel": "nature", "farm": "nature",
    "barn": "nature", "cowshed": "nature", "pigsty": "nature", "apiary": "nature",
    "fishpond": "nature", "coral_reef": "nature_water", "islet": "nature_water",
    "river": "nature", "lake": "nature", "ocean": "nature_water", "sea": "nature_water",
    "lagoon": "nature_water", "bayou": "nature_water", "delta": "nature_water",
    "estuary": "nature_water", "fjord": "nature_water", "reef": "nature_water",
    "shoal": "nature_water", "geyser": "nature", "hot_spring": "nature", "rock_arch": "nature",
    "cliff": "nature", "dune": "nature", "quarry": "industrial", "mine": "industrial",
    "coal_miner": "industrial", "oil_rig": "industrial", "power_plant": "industrial",
    "nuclear_power_plant": "industrial", "wind_farm": "nature",
    "industrial_area": "industrial", "factory": "industrial", "warehouse": "industrial",
    "water_tower": "urban", "windmill": "nature", "tower_fortification": "historic",
    "dump": "industrial", "junkyard": "industrial", "salvage_yard": "industrial",
    "construction_tool": "urban", "scaffolding": "urban", "foundry": "industrial",
    "printing_press": "industrial", "bleachers": "sports_stadium",
    "railway": "public_transport", "railroad_track": "public_transport",
    "dock": "nature", "wharf": "nature", "levant": "urban_detail",
    "canyon": "nature", "crevasse": "nature", "gorge": "nature",
    "slum": "urban", "shantytown": "urban", "artists_loft": "indoor_home",
    "studio": "indoor_home", "musical_instrument_studio": "indoor_home",
    "television_studio": "indoor_home", "film_set": "workplace", "stage": "indoor_cultural",
    "backstage": "indoor_cultural", "dressing_room": "indoor_cultural", "control_room": "workplace",
    "broadcast_studio": "workplace", "server_room": "workplace",
    "garage": "indoor_home", "parking_garage": "urban", "avalanche": "nature",
    "castleruin": "historic", "cliff": "nature", "cavern": "nature", "cave": "nature",
    "ice_flow": "nature", "ice_berg": "nature_water", "labyrinth": "nature",
    "monument": "historic", "prehistoric_site": "historic",
    "tundra": "nature", "steppe": "nature", "savanna": "nature", "prairie": "nature",
    "plains": "nature", "glacier": "nature", "moraine": "nature", "palsa": "nature",
    "rocky_area": "nature", "badlands": "nature", "mesa": "nature", "mangrove": "nature",
    "bog": "nature", "fen": "nature", "moor": "nature", "peat_bog": "nature", "taiga": "nature",
    "deciduous_forest": "nature", "coniferous_forest": "nature",
    "mushroom_farm": "nature", "winery": "food_dining", "brewery": "food_dining",
    "coffee_plantation": "nature", "tea_plantation": "nature", "sugarcane_field": "nature",
    "wheat_field": "nature", "wheat": "nature", "barley_field": "nature", "oat_field": "nature",
    "rye_field": "nature", "sunflower_field": "nature",
    "picnic_area": "nature", "scenic_overlook": "nature",
    "campsite": "nature", "campground": "nature",
    "night_street": "urban", "cityscape": "urban", "skyline": "cityscape",
    "elevator_interior": "indoor", "elevator_shaft": "indoor",
    "preschool": "education", "summer_camp": "sports", "school_cafeteria": "education",
    "high_school": "education", "university_lecture_hall": "education",
    "newsroom": "workplace", "radio_studio": "workplace",
    "science_museum": "indoor_cultural", "history_museum": "indoor_cultural",
    "museum": "indoor_cultural", "planetarium": "indoor_cultural",
    "aqueduct": "historic", "railroad_track": "public_transport", "tram_track": "public_transport",
    "track": "sports", "motocross": "sports", "camel_farm": "nature",
    "animal_shelter": "nature", "lion_snacks": "food_dining",
    "nap_area": "indoor_home", "rest_area": "nature", "inn": "hotel",
    "motel": "hotel", "hostel": "hotel", "bed_and_breakfast": "hotel",
    "cellar": "indoor_home", "vault": "indoor_home", "bunker": "indoor_home",
    "marine_terminus": "nature_water", "bus_depot": "public_transport",
    "construction_site": "urban", "interstate_highway": "urban", "highway": "urban",
    "crosswalk": "urban", "railroad_track": "public_transport",
    "city": "urban", "downtown": "urban", "village": "urban", "campus": "urban",
    "boardwalk": "urban", "viaduct": "urban", "driveway": "residential",
    "courthouse": "workplace", "lobby": "indoor", "auditorium": "indoor_cultural",
    "archive": "indoor_cultural", "recreation room": "indoor_home",
    "cockpit": "transport_vehicle", "raft": "nature_water", "wave": "nature_water",
    "creek": "nature", "dam": "nature", "lawn": "nature", "sky": "nature",
    "butte": "nature", "arch": "nature", "fountain": "urban", "tower": "historic",
    "pavilion": "urban", "porch": "residential", "patio": "residential",
    "yard": "residential", "shed": "residential", "tree house": "residential",
    "cottage": "residential", "mansion": "residential", "boathouse": "nature",
    "botanical garden": "nature", "cemetery": "urban", "boxing ring": "sports",
    "delicatessen": "food_dining", "rope bridge": "nature",
}

def normalize(name: str) -> str:
    n = name.strip().lstrip("/").replace("_", " ").lower()
    return re.sub(r"[^a-z0-9 ]+", " ", n)

def load_oi_classes(desc_path: pathlib.Path, trainable_path: pathlib.Path) -> list[tuple[str, str, str]]:
    trainable = {t.strip() for t in trainable_path.read_text().splitlines() if t.strip()}
    out = []
    with open(desc_path, newline="") as f:
        for mid, name in csv.reader(f):
            if mid in trainable:
                out.append((mid, name, normalize(name)))
    return out

def load_places365(path: pathlib.Path) -> list[tuple[str, str]]:
    out = []
    for line in path.read_text().splitlines():
        line = line.strip()
        if not line:
            continue
        raw = line.lstrip("/")
        raw = re.sub(r"\s+\d+$", "", raw)  # trailing index column
        parts = raw.split("/")
        # Format is /b/beach or /f/forest/broadleaf: the first segment is a
        # single-letter prefix (first char of the class name); drop it and
        # join the rest.
        label = "_".join(parts[1:]) if len(parts) > 1 and len(parts[0]) == 1 else "_".join(parts)
        label = label.replace("_", " ").strip()
        out.append((label, normalize(label)))
    return out

def best_match(place_norm: str, oi: list[tuple[str, str, str]], cutoff: float = 0.8):
    """Token-aware fuzzy: a candidate must share a real word with the places
    label (kills 'alcove -> Love' style junk), then SequenceMatcher ratio."""
    p_tokens = {w for w in place_norm.split() if len(w) > 2}
    candidates = []
    for mid, name, norm in oi:
        if not p_tokens:
            continue
        n_tokens = {w for w in norm.split() if len(w) > 2}
        shared = p_tokens & n_tokens
        # require >= 1 shared token, or exact substring containment
        if shared or place_norm in norm or norm in place_norm:
            candidates.append((mid, name, norm))
    if not candidates:
        return None
    best = max(candidates, key=lambda c: difflib.SequenceMatcher(None, place_norm, c[2]).ratio())
    score = difflib.SequenceMatcher(None, place_norm, best[2]).ratio()
    if score < cutoff:
        return None
    return best[0], best[1], score

def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--meta", default="ml-corpus/openimages/metadata")
    ap.add_argument("--map", default="tools/train/class-map.json")
    args = ap.parse_args()
    meta = pathlib.Path(args.meta)
    oi = load_oi_classes(meta / "class-descriptions.csv", meta / "classes-trainable.txt")
    print(f"OI trainable classes: {len(oi)}")
    places = load_places365(meta / "categories_places365.txt")
    print(f"Places365 names: {len(places)}")

    by_norm: dict[str, tuple[str, str]] = {n: (m, name) for m, name, n in oi}
    exact, fuzzy, missed = [], [], []
    for label, norm in places:
        if norm in by_norm:
            mid, name = by_norm[norm]
            exact.append((label, norm, mid, name, 1.0))
        else:
            hit = best_match(norm, oi)
            if hit:
                mid, name, score = hit
                fuzzy.append((label, norm, mid, name, score))
            else:
                missed.append((label, norm))

    print(f"exact: {len(exact)}  fuzzy: {len(fuzzy)}  missed: {len(missed)}")

    # Manual overrides for well-known mismatches found during the first review.
    OVERRIDES = {
        "bazaar indoor": ("/m/0d_l32", "bazaar"),  # placeholder — review will fix
    }

    # Hand-curated (2026-08-18 review of the 0.70-0.80 band): places labels
    # whose best token-shared OI class is genuinely the same scene.
    FUZZY_PLUS = {
        "amusement arcade": "Amusement ride",
        "martial arts gym": "Martial arts",
        "mountain path": "Mountain pass",
        "home theater": "Movie theater",
        "movie theater indoor": "Movie theater",
        "shopping mall indoor": "Shopping mall",
        "swimming pool indoor": "Swimming pool",
        "swimming pool outdoor": "Swimming pool",
        "restaurant patio": "Restaurant",
        "flea market indoor": "Flea market",
        "hospital room": "Hospital",
        "ski resort": "Resort",
        "zen garden": "Garden",
        "auto factory": "Factory",
        "banquet hall": "Banquet",
        "greenhouse indoor": "Greenhouse",
        "shoe shop": "Shoe store",
        "train station platform": "Train station",
        "conference room": "Conference hall",
        "construction site": "Construction",
    }
    by_name = {name.lower(): (mid, name) for mid, name, _ in oi}
    for label, oi_name in FUZZY_PLUS.items():
        hit = by_name.get(oi_name.lower())
        if hit:
            mid, name = hit
            norm = normalize(name)
            score = difflib.SequenceMatcher(None, normalize(label), norm).ratio()
            fuzzy.append((label, norm, mid, name, score))

    mapping = {}
    for label, norm, mid, name, score in exact + fuzzy:
        mapped_name = name.replace(" ", "_").lower()
        coarse = COARSE_GROUPS.get(mapped_name, "other")
        mapping[label] = {"mid": mid, "oi_name": name, "score": round(score, 3),
                          "coarse": coarse}

    # Hand-curated after the first mapping review: places labels that must be
    # dropped (no trustworthy OI equivalent) or overridden (better MID).
    DROPS = {
        "alcove", "berth", "hot spring", "tree farm", "fire station",
        "grotto", "hayfield", "orchard", "computer room", "clean room",
        "excavation", "ice shelf", "medina", "oast house", "phone booth",
        "swimming hole", "television studio", "auto showroom", "orchestra pit",
        "arena performance", "operating room", "reception", "martial arts gym",
        "corral", "galley", "mountain path", "ruin", "amusement arcade",
        "garage indoor", "garage outdoor", "construction site", "desert road",
        "motel", "staircase", "television room", "airfield", "oilrig",
        "carrousel", "fishpond", "amphitheater", "home theater",
        "movie theater indoor", "shopping mall indoor", "swimming pool indoor",
        "orchestra pit", "fastfood restaurant",
    }
    OVERRIDES: dict[str, tuple[str, str]] = {
        # places label -> (mid, exact OI name) — MIDs verified against
        # class-descriptions.csv
        "church": ("/m/01wb7", "Church"),
        "desert": ("/m/0284w", "Desert"),
        "city": ("/m/01n32", "City"),
        "north church": ("/m/01wb7", "Church"),
    }
    for label in DROPS:
        mapping.pop(label, None)
    for label, (mid, name) in OVERRIDES.items():
        norm = normalize(name)
        coarse = COARSE_GROUPS.get(name.replace(" ", "_").lower(), "other")
        mapping[label] = {"mid": mid, "oi_name": name, "score": 1.0, "coarse": coarse}
        if (label, norm, mid, name, 1.0) not in exact and (label, norm, mid, name, 1.0) not in fuzzy:
            exact.append((label, norm, mid, name, 1.0))

    review = meta / "map_review.txt"
    with open(review, "w") as f:
        f.write("MISSED (no good OI class) — decide: drop or hand-map\n")
        f.writelines(f"  {l} ({n})\n" for l, n in sorted(missed))
        f.write("\nFUZZY (check these)\n")
        f.writelines(f"  {l} -> {m} {n} ({s:.2f})\n" for l, _, m, n, s in sorted(fuzzy, key=lambda t: -t[4]))
    print(f"mapped: {len(mapping)}  missed: {len(missed)}  review: {review}")

    out = pathlib.Path(args.map)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(mapping, indent=2, sort_keys=True) + "\n")
    print(f"wrote {out}")

if __name__ == "__main__":
    main()