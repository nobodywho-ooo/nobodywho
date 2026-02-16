import typing

import nobodywho as nw
from pydantic import dataclasses



cw1 = """
The following is a mini crossword.

The blanks represent spaces to fill with letters.
The blanks are annotated with a coordinate system,
giving each blank square a coordinate.
For each clue, a starting coordinate, a direction, and a number of letters is given.

0123456
1  ___
2 _____
3_______
4___ ___
5___ ___
6 _____
7 _____

Coord: (1,3), Length: 3, Direction: across, Clue: "Fashionable"
Coord: 


"""

# The following is a mini crossword.
#
# The blanks represent spaces to fill with letters.
# The blanks are annotated with a coordinate system,
# giving each blank square a coordinate.
# For each clue, a starting coordinate, a direction, and a number of letters is given.
cw2 = """

 12345
1  ___
2 ____
3_____
4____
5___


Coord: 3,1, Length: 3, Dir: across, Clue: "___ King Cole, Singer with the album "The Magic of Christmas"
Coord: 2,2, Length: 4, Dir: across, Clue: "Body drawings, informally"
Coord: 1,3, Length: 5, Dir: across, Clue: "Letters to ___ (what this Mini was made with)"
Coord: 1,4, Length: 4, Dir: across, Clue: "Huge fan, in slang"
Coord: 1,5, Length: 3, Dir: across, Clue: "'Illmatic' rapper"

Coord: 3,1, Length: 5, Dir: down, Clue: "Grandmothers, by another name"
Coord: 4,1, Length: 4, Dir: down, Clue: "Abbr. before a name on a memo"
Coord: 5,1, Length: 3, Dir: down, Clue: "Org. with long lines around the holidays"
Coord: 2,2, Length: 4, Dir: down, Clue: "See ya later!"
Coord: 1,3, Length: 3, Dir: down, Clue: "Govt.-issued ID"

Please solve the crossword.
"""


system_prompt = """
You are an expert crossword solver.

The following is a New York Times "Mini" crossword.

The blanks represent spaces to fill with letters.
The blanks are annotated with a coordinate system,
giving each blank square a coordinate.
For each clue, a starting coordinate, a direction, and a number of letters is given.
The coordinates are (x,y), aka (column, row).
"""

@dataclasses.dataclass
class Clue:
    clue_text: str
    direction: typing.Literal["across", "down"]
    coord: tuple[int, int]
    length: int


clues = [
    # Across clues
    Clue(
        clue_text='___ King Cole, Singer with the album "The Magic of Christmas"',
        direction="across",
        coord=(3, 1),
        length=3,
    ),
    Clue(
        clue_text="Body drawings, informally",
        direction="across",
        coord=(2, 2),
        length=4,
    ),
    Clue(
        clue_text="Letters to ___ (what this Mini was made with)",
        direction="across",
        coord=(1, 3),
        length=5,
    ),
    Clue(clue_text="Huge fan, in slang", direction="across", coord=(1, 4), length=4),
    Clue(clue_text="'Illmatic' rapper", direction="across", coord=(1, 5), length=3),
    # Down clues
    Clue(
        clue_text="Grandmothers, by another name",
        direction="down",
        coord=(3, 1),
        length=5,
    ),
    Clue(
        clue_text="Abbr. before a name on a memo",
        direction="down",
        coord=(4, 1),
        length=4,
    ),
    Clue(
        clue_text="Org. with long lines around the holidays",
        direction="down",
        coord=(5, 1),
        length=3,
    ),
    Clue(clue_text="See ya later!", direction="down", coord=(2, 2), length=4),
    Clue(clue_text="Govt.-issued ID", direction="down", coord=(1, 3), length=3),
]


@nw.tool(
    description="Check whether a word is a valid guess, in a given position and orientation.",
    params={
        "col": "column number of the first character of the word",
        "row": "row number of the first character of the word",
        "dir": "direction of the word, must be either 'across' or 'down'",
        "word": "the actual word to guess",
    },
)
def check_word(col: int, row: int, dir: str, word: str) -> str:
    # check dir
    dir = dir.lower()
    if dir != "across" and dir != "down":
        return "Invalid dir: must be 'across' or 'down'"

    # search for clue
    theclue = None
    for clue in clues:
        if col == clue.coord[0] and row == clue.coord[1] and clue.direction == dir:
            theclue = clue

    # check find
    if theclue is None:
        return "The clue was not found"
    assert isinstance(theclue, Clue)

    # check length
    if len(word) != theclue.length:
        return f"Length was wrong. Should have been {theclue.length}, was {len(word)}"

    # TODO: check conflicts with previously submitted words

    # 200 OK
    return "The word fits here. It's not necessarily correct, but it fits."


sampler = nw.SamplerBuilder().temperature(0.6).top_k(20).top_p(0.95, min_keep=20).dist()

model_path = "./Qwen_Qwen3-4B-Q4_K_M.gguf"

chat = nw.Chat(
    model_path,
    system_prompt=system_prompt,
    n_ctx=8192,
    sampler=sampler,
    tools=[check_word],
)
response = chat.ask(f"Please solve this crossword: {cw2}")

for token in response:
    print(token, end="", flush=True)
