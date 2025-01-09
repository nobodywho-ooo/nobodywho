extends NobodyWhoEmbedding

func run_test():
	# configure node
	self.model_node = get_node("../EmbeddingModel")

	# generate some embeddings
	embed("The dragon is on the hill.")
	var dragon_hill_embd = await self.embedding_finished

	embed("The dragon is hungry for humans.")
	var dragon_hungry_embd = await self.embedding_finished

	embed("This doesn't matter.")
	var irrelevant_embd = await self.embedding_finished

	# test similarity
	var low_similarity = cosine_similarity(irrelevant_embd, dragon_hill_embd)
	var high_similarity = cosine_similarity(dragon_hill_embd, dragon_hungry_embd) 
	var result = low_similarity < high_similarity
	assert(result)
	print("✨ embeddings completed")
	return result
