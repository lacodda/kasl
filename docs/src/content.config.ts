import { defineCollection } from 'astro:content';
import { docsLoader, i18nLoader } from '@astrojs/starlight/loaders';
import { docsSchema, i18nSchema } from '@astrojs/starlight/schema';

export const collections = {
	docs: defineCollection({ loader: docsLoader(), schema: docsSchema() }),
	// Declared so Starlight's lookup finds a collection rather than warning on every build; the site has one language.
	i18n: defineCollection({ loader: i18nLoader(), schema: i18nSchema() }),
};
