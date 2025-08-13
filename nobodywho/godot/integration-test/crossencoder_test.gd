extends NobodyWhoCrossEncoder

func run_test():
	# configure node
	self.model_node = get_node("../CrossEncoderModel")
	
	# test ranking documents
	var query = "What is the capital of France?"
	var documents = PackedStringArray([
		"The Eiffel Tower is a famous landmark in the capital of France.",
		"France is a country in Europe.",
		"Lyon is a major city in France, but not the capital.",
		"The capital of Germany is France.",
		"The French government is based in Paris.",
		"France's capital city is known for its art and culture, it is called Paris.",
		"The Louvre Museum is located in Paris, France - which is the largest city, and the seat of the government",
		"Paris is the capital of France.",
		"Paris is not the capital of France.",
		"The president of France works in Paris, the main city of his country.",
		"What is the capital of France?"
	])
	
	# Test ranking with limit
	var ranked_docs: PackedStringArray = await rank(query, documents, 3)
	print("✨ Got ranked documents: " + str(ranked_docs))
	
	# Basic validation
	assert(ranked_docs.size() == 3, "Should return exactly 3 documents")
	assert("".join(ranked_docs).contains("Paris is the capital of France"), "Paris is the capital of France should be in the top 3")
	
	# Test ranking without limit (should return all documents)
	var all_ranked_docs = await rank(query, documents, -1)
	print("✨ Got all ranked documents: " + str(all_ranked_docs))
	
	assert(all_ranked_docs.size() == documents.size(), "Should return all documents when limit is -1")
	
	return true 
