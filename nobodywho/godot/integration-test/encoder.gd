extends NobodyWhoEncoder

func run_test():
	# configure node
	self.model_node = get_node("../EmbeddingModel")

	# generate some encodings
	encode("The dragon is on the hill.")
	var dragon_hill_enc = await self.encoding_finished

	encode("The dragon is hungry for humans.")
	var dragon_hungry_enc = await self.encoding_finished

	encode("This doesn't matter.")
	var irrelevant_enc = await self.encoding_finished

	# test similarity
	var low_similarity = cosine_similarity(irrelevant_enc, dragon_hill_enc)
	var high_similarity = cosine_similarity(dragon_hill_enc, dragon_hungry_enc) 
	var result = low_similarity < high_similarity
	assert(result)
	print("✨ encoder completed")
	return result
