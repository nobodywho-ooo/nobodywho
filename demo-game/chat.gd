extends NobodyWhoChat

@onready var potion_embeddings = await generate_potion_embeddings()
var selected_potion = null

func _ready() -> void:
	start_worker()

func _on_response_updated(new_token: String) -> void:
	%TextEdit.text += new_token

func _on_response_finished(response: String) -> void:
	%OkButton.disabled = false

func _on_send_button_pressed() -> void:
	var potion = await match_sentence(%TextEdit.text)
	if potion != null:
		confirm_buy_potion(potion)
	
	# send user text to the llm
	say(%TextEdit.text)
	
	# reset the input box
	%TextEdit.text = ""
	%TextEdit.editable = false
	
	# show the other button
	%SendButton.visible = false
	%OkButton.visible = true
	%OkButton.disabled = true

func _on_ok_button_pressed() -> void:
	%TextEdit.editable = true
	%TextEdit.text = ""
	%OkButton.visible = false
	%SendButton.visible = true

func confirm_buy_potion(potion: String):
	selected_potion = potion
	if potion == "health_potion":
		%ConfirmLabel.text = "Buy health potion for 3 gold?"
	elif potion == "mana_potion":
		%ConfirmLabel.text = "Buy mana potion for 3 gold?"
	elif potion == "strength_potion":
		%ConfirmLabel.text = "Buy strength potion for 5 gold?"
	else:
		assert(false) # unreachable, if we're ever here, it's a bug.
	%ConfirmBox.visible = true

func generate_potion_embeddings():
	# generate embeddings for a few example sentences for each potion
	# these will be what we use to detect if the user has asked to buy a potion
	var sentences = {
		"health_potion": [
			"I'd like to buy the potion of minor healing.",
			"I'd like to buy the potion in the round bottle.",
			"Give me the health potion.",
			"Buy the red potion."
		],
		"mana_potion": [
			"Give me the blue potion.",
			"I'll get that mana potion.",
			"I'd like the potion in the test tube.",
			"I will purchase the potion in the skinny flask"
		],
		"strength_potion": [
			"I'm gonna buy the strength potion.",
			"Give me the orange potion.",
			"I'd like the one in the square flask",
			"I will purchase the potion that gives extra damage"
		]
	}
	var embeddings = {"health_potion": [], "mana_potion": [], "strength_potion": []}
	for potion in sentences:
		for sentence in sentences[potion]:
			embeddings[potion].append(await %NobodyWhoEmbedding.embed(sentence))
	return embeddings

func match_sentence(sentence: String):
	# test if the user asked to buy a potion
	# returns the name of the potion if yes
	# returns null otherwise
	var max_similarity = 0
	var most_similar = null
	var input_embed = await %NobodyWhoEmbedding.embed(sentence)
	for potion in potion_embeddings:
		for embedding in potion_embeddings[potion]:
			var similarity = %NobodyWhoEmbedding.cosine_similarity(input_embed, embedding)
			print(potion + " " + str(similarity))
			if similarity > max_similarity:
				most_similar = potion
				max_similarity = similarity
	var threshold = 0.8
	if max_similarity > threshold:
		print("MOST SIMILAR: " + most_similar)
		return most_similar
	return null


func buy_selected_potion():
	if selected_potion == "health_potion":
		pass
	elif selected_potion == "mana_potion":
		pass
	elif selected_potion == "strength_potion":
		pass
	selected_potion = null

func _on_yes_button_pressed() -> void:
	buy_selected_potion()
	%ConfirmBox.visible = false


func _on_no_button_pressed() -> void:
	%ConfirmBox.visible = false
	selected_potion = false
