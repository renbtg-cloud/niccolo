import { z } from 'zod';
export const connectionSchema = z.object({
  workspace: z.string().uuid(),
  situation: z.string().uuid(),
  token: z.string().min(1),
});
export const analysisSchema = z.object({ analysis: z.string().uuid() });
